import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { ChevronDown, LoaderCircle, Puzzle, X } from "lucide-react";
import { Modal } from "./App";
import { errorMessage } from "./api";
import type { PluginController } from "./usePlugins";

export function PluginCommandMenu({ controller, componentId }: { controller: PluginController; componentId?: string }) {
  const commands = controller.items.filter((plugin) => plugin.enabled && plugin.trusted && plugin.compatible).flatMap((plugin) =>
    plugin.manifest.commands.filter((command) => command.menus.includes(componentId ? "component" : "global")).map((command) => ({ plugin, command })));
  if (!commands.length) return null;
  return <Dropdown.Root><Dropdown.Trigger asChild><button className={componentId ? "plugin-component-menu" : "button secondary"} aria-label={componentId ? "组件插件操作" : "插件工具"} title={componentId ? "组件插件操作" : "插件工具"} disabled={!!controller.busy}><Puzzle size={16} />{!componentId && <><span>插件工具</span><ChevronDown size={13} /></>}</button></Dropdown.Trigger><Dropdown.Portal><Dropdown.Content className="plugin-menu-content" align="end" sideOffset={6}>
    {commands.map(({ plugin, command }) => <Dropdown.Item className="plugin-menu-item" key={`${plugin.manifest.id}:${command.id}`} onSelect={() => controller.requestCommand(plugin, command, componentId)}><span>{command.title}</span><small>{plugin.manifest.name}</small></Dropdown.Item>)}
  </Dropdown.Content></Dropdown.Portal></Dropdown.Root>;
}

export function PluginFeedback({ controller, managerVisible }: { controller: PluginController; managerVisible: boolean }) {
  return <>
    {!managerVisible && (controller.busy || controller.error) && <div className="plugin-feedback">
      {controller.busy && <div className="plugin-task" role="status"><LoaderCircle className="spin" size={17} /><span>{controller.busy.title}</span><button className="icon-button" title="取消插件任务" aria-label="取消插件任务" onClick={() => void controller.cancel().catch((cause) => controller.setError(errorMessage(cause)))}><X size={16} /></button></div>}
      {controller.error && <div className="plugin-error" role="alert"><span>{controller.error}</span><button className="icon-button" title="关闭插件错误" aria-label="关闭插件错误" onClick={() => controller.setError("")}><X size={15} /></button></div>}
    </div>}
    {controller.result && <Modal title={controller.result.title} onClose={() => controller.setResult(null)}><pre className="plugin-command-result">{typeof controller.result.value === "string" ? controller.result.value : JSON.stringify(controller.result.value, null, 2)}</pre><div className="dialog-actions"><button className="button secondary" onClick={() => controller.setResult(null)}>关闭</button></div></Modal>}
  </>;
}
