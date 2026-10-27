import { useState, type FormEvent } from "react";
import { Play, X } from "lucide-react";
import { Modal } from "./App";
import type { Json, JsonSchema } from "./pluginTypes";
import type { PluginController } from "./usePlugins";

export function schemaDefaults(schema: JsonSchema): Record<string, Json> {
  return Object.fromEntries(Object.entries(schema.properties ?? {}).flatMap(([name, field]) => {
    const value = field.default ?? (schema.required?.includes(name) ? field.enum?.[0] ?? (field.type === "boolean" ? false : undefined) : undefined);
    return value === undefined ? [] : [[name, value]];
  }));
}

export function SchemaFields({ schema, value, onChange, disabled }: { schema: JsonSchema; value: Record<string, Json>; onChange: (value: Record<string, Json>) => void; disabled?: boolean }) {
  return <div className="plugin-fields">{Object.entries(schema.properties ?? {}).map(([name, field]) => {
    const id = `plugin-field-${name}`;
    const set = (next: Json) => onChange({ ...value, [name]: next });
    return <label className={field.type === "boolean" ? "plugin-checkbox" : "plugin-field"} key={name} htmlFor={id}>
      {field.type === "boolean" ? <><input id={id} type="checkbox" checked={value[name] === true} disabled={disabled} onChange={(event) => set(event.target.checked)} /><span>{field.title ?? name}</span></> : <>
        <span>{field.title ?? name}{schema.required?.includes(name) ? " *" : ""}</span>
        {field.enum ? <select id={id} disabled={disabled} value={JSON.stringify(value[name] ?? field.enum[0])} onChange={(event) => set(JSON.parse(event.target.value) as Json)}>{field.enum.map((option) => <option key={JSON.stringify(option)} value={JSON.stringify(option)}>{String(option)}</option>)}</select> :
          field.type === "number" || field.type === "integer" ? <input id={id} type="number" required={schema.required?.includes(name)} step={field.type === "integer" ? 1 : "any"} min={field.minimum} max={field.maximum} value={typeof value[name] === "number" ? value[name] : ""} disabled={disabled} onChange={(event) => {
            if (event.target.value === "") { const next = { ...value }; delete next[name]; onChange(next); }
            else set(Number(event.target.value));
          }} /> : field.type === "object" || field.type === "array" ? <JsonInput id={id} value={value[name] ?? (field.type === "array" ? [] : {})} disabled={disabled} onChange={set} /> :
          <input id={id} required={schema.required?.includes(name)} disabled={disabled} value={String(value[name] ?? "")} onChange={(event) => set(event.target.value)} />}
      </>}
    </label>;
  })}</div>;
}

function JsonInput({ id, value, disabled, onChange }: { id: string; value: Json; disabled?: boolean; onChange: (value: Json) => void }) {
  const [text, setText] = useState(JSON.stringify(value, null, 2));
  return <textarea id={id} value={text} disabled={disabled} spellCheck={false} rows={4} onChange={(event) => {
    setText(event.target.value);
    try { onChange(JSON.parse(event.target.value) as Json); event.target.setCustomValidity(""); }
    catch { event.target.setCustomValidity("请输入有效的 JSON。"); }
  }} />;
}

export function PluginCommandPrompt({ controller }: { controller: PluginController }) {
  const prompt = controller.prompt;
  return prompt ? <CommandForm key={`${prompt.plugin.fingerprint}:${prompt.command.id}`} controller={controller} /> : null;
}
function CommandForm({ controller }: { controller: PluginController }) {
  const prompt = controller.prompt!;
  const [input, setInput] = useState(schemaDefaults(prompt.command.input_schema));
  const submit = (event: FormEvent) => { event.preventDefault(); void controller.execute(prompt.plugin, prompt.command, input, prompt.componentId); };
  return <Modal title={prompt.command.title} subtitle={prompt.plugin.manifest.name} onClose={() => controller.setPrompt(null)}><form onSubmit={submit}>
    <SchemaFields schema={prompt.command.input_schema} value={input} onChange={setInput} />
    <div className="dialog-actions"><button type="button" className="button secondary" onClick={() => controller.setPrompt(null)}><X size={15} />取消</button><button className="button primary" type="submit"><Play size={15} />执行</button></div>
  </form></Modal>;
}
