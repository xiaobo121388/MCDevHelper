import * as Dropdown from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";

export type McdkChoiceItem = { value: string; label: string; badge?: string; description?: string; icon?: ReactNode };
export type McdkChoiceGroup = { label?: string; items: McdkChoiceItem[] };

export function McdkChoice({ label, value, groups, disabled = false, onChange }: {
  label: string; value: string; groups: McdkChoiceGroup[]; disabled?: boolean; onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  const selected = groups.flatMap(group => group.items).find(item => item.value === value);
  return <Dropdown.Root modal={false} open={open && !disabled} onOpenChange={setOpen}>
    <Dropdown.Trigger asChild><button type="button" className="mcdk-choice-trigger" aria-label={label} disabled={disabled} title={selected?.description}>
      {selected?.icon && <span className="mcdk-choice-icon" aria-hidden="true">{selected.icon}</span>}
      <span className="mcdk-choice-text"><span>{selected?.label ?? "未选择"}</span>{selected?.badge && <small>{selected.badge}</small>}</span><ChevronDown size={15} />
    </button></Dropdown.Trigger>
    <Dropdown.Portal><Dropdown.Content className="mcdk-choice-menu" align="start" sideOffset={5} collisionPadding={10} aria-label={label}>
      <Dropdown.RadioGroup value={value} onValueChange={next => { if (!disabled) onChange(next); }}>
        {groups.filter(group => group.items.length).map((group, index) => <Dropdown.Group key={group.label ?? index}>
          {group.label && <Dropdown.Label className="mcdk-choice-group">{group.label}<span>{group.items.length}</span></Dropdown.Label>}
          {group.items.map(item => <Dropdown.RadioItem key={item.value} value={item.value} textValue={item.label} className="mcdk-choice-item" aria-label={item.label + (item.badge ? " · " + item.badge : "")}>
            <span className="mcdk-choice-icon" aria-hidden="true">{item.icon}</span>
            <span className="mcdk-choice-copy"><span className="mcdk-choice-text"><span>{item.label}</span>{item.badge && <small>{item.badge}</small>}</span>{item.description && <span className="mcdk-choice-description" title={item.description}>{item.description}</span>}</span>
            <span className="mcdk-choice-indicator"><Dropdown.ItemIndicator><Check size={16} /></Dropdown.ItemIndicator></span>
          </Dropdown.RadioItem>)}
        </Dropdown.Group>)}
      </Dropdown.RadioGroup>
    </Dropdown.Content></Dropdown.Portal>
  </Dropdown.Root>;
}
