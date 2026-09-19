using System;
using System.IO;
using System.Threading;

class UpdateFixture {
    static int Main(string[] args) {
        string root = AppDomain.CurrentDomain.BaseDirectory.TrimEnd(Path.DirectorySeparatorChar);
        if (args.Length > 0 && args[0] == "--wait") {
            File.WriteAllText(Path.Combine(root, "parent-started"), "ready");
            while (!File.Exists(Path.Combine(root, "exit-parent"))) Thread.Sleep(50);
            return 0;
        }
        if (Path.GetFileName(Environment.GetCommandLineArgs()[0]).Equals("setup.exe", StringComparison.OrdinalIgnoreCase)) {
            string command = Environment.CommandLine;
            File.WriteAllText(Path.Combine(root, "installer-args.txt"), command);
            int offset = command.IndexOf("/D=", StringComparison.Ordinal);
            if (!command.Contains("/S /UPDATE /D=") || offset < 0) return 9;
            string target = command.Substring(offset + 3);
            if (target != Directory.GetParent(root).FullName) return 10;
            string app = Path.Combine(target, "mcdh-desktop.exe");
            File.WriteAllText(Path.Combine(target, "mcdh-mcp.exe"), "new-mcp");
            if (File.Exists(Path.Combine(root, "fail-installer"))) {
                File.WriteAllText(app, "corrupted-app");
                return 7;
            }
            File.Copy(Path.Combine(root, "new-app.exe"), app, true);
            return 0;
        }
        File.AppendAllText(Path.Combine(root, "restarted.txt"), "started\n");
        return 0;
    }
}
