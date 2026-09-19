use crate::Result;
use std::path::Path;

#[cfg(windows)]
pub(super) use windows::ExportProcess;

#[cfg(not(windows))]
pub(super) struct ExportProcess {
    pub stdout: Option<std::fs::File>,
    pub stderr: Option<std::fs::File>,
}

#[cfg(not(windows))]
impl ExportProcess {
    pub fn start(_: &Path, _: &[String], _: &Path, _: &[(String, String)]) -> Result<Self> {
        Err(super::files::failure(
            "start_failed",
            "自定义导出执行器仅支持 Windows",
        ))
    }
    pub fn poll(&self) -> Result<Option<u32>> {
        Ok(Some(1))
    }
    pub fn terminate(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use super::super::files::failure;
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    use std::fs::File;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation, WAIT_OBJECT_0},
        Security::SECURITY_ATTRIBUTES,
        System::{JobObjects::*, Pipes::CreatePipe, Threading::*},
    };

    fn last_error() -> crate::CoreError {
        failure("start_failed", std::io::Error::last_os_error().to_string())
    }
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    // Microsoft CRT argv escaping, not shell syntax. The executable is passed separately.
    fn quote(value: &str) -> String {
        let mut result = String::from("\"");
        let mut slashes = 0;
        for c in value.chars() {
            if c == '\\' {
                slashes += 1;
                continue;
            }
            if c == '"' {
                result.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
            } else {
                result.extend(std::iter::repeat_n('\\', slashes));
            }
            slashes = 0;
            result.push(c);
        }
        result.extend(std::iter::repeat_n('\\', slashes * 2));
        result.push('"');
        result
    }

    fn pipe() -> Result<(OwnedHandle, OwnedHandle)> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (null_mut(), null_mut());
        // Both returned handles are immediately owned, including on later initialization failure.
        unsafe {
            if CreatePipe(&mut read, &mut write, &attributes, 0) == 0 {
                return Err(last_error());
            }
            Ok((
                OwnedHandle::from_raw_handle(read),
                OwnedHandle::from_raw_handle(write),
            ))
        }
    }

    struct Attributes {
        _storage: Vec<usize>,
        pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
    }
    impl Drop for Attributes {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer);
            }
        }
    }

    impl Attributes {
        fn new(
            handles: &mut [*mut std::ffi::c_void],
            jobs: &mut [*mut std::ffi::c_void],
        ) -> Result<Self> {
            let mut bytes = 0;
            unsafe {
                InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
                let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
                let pointer = storage.as_mut_ptr().cast();
                if InitializeProcThreadAttributeList(pointer, 2, 0, &mut bytes) == 0 {
                    return Err(last_error());
                }
                let attributes = Self {
                    _storage: storage,
                    pointer,
                };
                if UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_mut_ptr().cast(),
                    std::mem::size_of_val(handles),
                    null_mut(),
                    null(),
                ) == 0
                {
                    return Err(last_error());
                }
                if UpdateProcThreadAttribute(
                    pointer,
                    0,
                    PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                    jobs.as_mut_ptr().cast(),
                    std::mem::size_of_val(jobs),
                    null_mut(),
                    null(),
                ) == 0
                {
                    return Err(last_error());
                }
                Ok(attributes)
            }
        }
    }

    fn create_job() -> Result<OwnedHandle> {
        unsafe {
            let raw_job = CreateJobObjectW(null(), null());
            if raw_job.is_null() {
                return Err(last_error());
            }
            let job = OwnedHandle::from_raw_handle(raw_job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(last_error());
            }
            Ok(job)
        }
    }

    pub(crate) struct ExportProcess {
        job: OwnedHandle,
        process: OwnedHandle,
        pub stdout: Option<File>,
        pub stderr: Option<File>,
    }

    impl ExportProcess {
        pub fn start(
            executable: &Path,
            arguments: &[String],
            cwd: &Path,
            variables: &[(String, String)],
        ) -> Result<Self> {
            let job = create_job()?;
            let (out_read, out_write) = pipe()?;
            let (err_read, err_write) = pipe()?;
            let (in_read, in_write) = pipe()?;
            drop(in_write);
            unsafe {
                if SetHandleInformation(out_read.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) == 0
                    || SetHandleInformation(err_read.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) == 0
                {
                    return Err(last_error());
                }
            }
            let mut handles = [
                out_write.as_raw_handle(),
                err_write.as_raw_handle(),
                in_read.as_raw_handle(),
            ];
            let mut jobs = [job.as_raw_handle()];
            let attributes = Attributes::new(&mut handles, &mut jobs)?;
            let mut environment = BTreeMap::new();
            for (key, value) in std::env::vars_os() {
                environment.insert(key.to_string_lossy().to_uppercase(), (key, value));
            }
            for (key, value) in variables {
                environment.insert(key.to_uppercase(), (key.into(), value.into()));
            }
            let mut block = Vec::new();
            for (_, (key, value)) in environment {
                block.extend(key.encode_wide());
                block.push('=' as u16);
                block.extend(value.encode_wide());
                block.push(0);
            }
            block.push(0);
            let mut command = quote(&executable.to_string_lossy());
            for arg in arguments {
                command.push(' ');
                command.push_str(&quote(arg));
            }
            let mut command = wide(OsStr::new(&command));
            if command.len() > 32767 {
                return Err(failure("start_failed", "命令行超过 Windows 长度限制"));
            }
            let executable = wide(executable.as_os_str());
            let cwd = wide(cwd.as_os_str());
            // Atomic job assignment prevents host crashes from stranding a suspended child.
            unsafe {
                let mut startup: STARTUPINFOEXW = zeroed();
                startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
                startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
                startup.StartupInfo.hStdOutput = out_write.as_raw_handle();
                startup.StartupInfo.hStdError = err_write.as_raw_handle();
                startup.StartupInfo.hStdInput = in_read.as_raw_handle();
                startup.lpAttributeList = attributes.pointer;
                let mut info: PROCESS_INFORMATION = zeroed();
                if CreateProcessW(
                    executable.as_ptr(),
                    command.as_mut_ptr(),
                    null(),
                    null(),
                    1,
                    CREATE_SUSPENDED
                        | CREATE_NO_WINDOW
                        | CREATE_UNICODE_ENVIRONMENT
                        | EXTENDED_STARTUPINFO_PRESENT,
                    block.as_ptr().cast(),
                    cwd.as_ptr(),
                    &startup.StartupInfo,
                    &mut info,
                ) == 0
                {
                    return Err(last_error());
                }
                let process = OwnedHandle::from_raw_handle(info.hProcess);
                let thread = OwnedHandle::from_raw_handle(info.hThread);
                let result = Self {
                    job,
                    process,
                    stdout: Some(out_read.into()),
                    stderr: Some(err_read.into()),
                };
                if ResumeThread(thread.as_raw_handle()) == u32::MAX {
                    return Err(last_error());
                }
                Ok(result)
            }
        }

        pub fn poll(&self) -> Result<Option<u32>> {
            unsafe {
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = zeroed();
                if QueryInformationJobObject(
                    self.job.as_raw_handle(),
                    JobObjectBasicAccountingInformation,
                    (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of_val(&accounting) as u32,
                    null_mut(),
                ) == 0
                {
                    return Err(last_error());
                }
                if accounting.ActiveProcesses != 0 {
                    return Ok(None);
                }
                if WaitForSingleObject(self.process.as_raw_handle(), 0) != WAIT_OBJECT_0 {
                    return Ok(None);
                }
                let mut code = 0;
                if GetExitCodeProcess(self.process.as_raw_handle(), &mut code) == 0 {
                    return Err(last_error());
                }
                Ok(Some(code))
            }
        }

        pub fn terminate(&self) -> Result<()> {
            unsafe {
                if TerminateJobObject(self.job.as_raw_handle(), 1) == 0 {
                    return Err(last_error());
                }
            }
            while self.poll()?.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(())
        }
    }

    impl Drop for ExportProcess {
        fn drop(&mut self) {
            let _ = self.terminate();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn quotes_literal_argv() {
            assert_eq!(quote(""), "\"\"");
            assert_eq!(quote("a b\\"), "\"a b\\\\\"");
            assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        }
    }
}
