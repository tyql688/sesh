import { createSignal } from "solid-js";

const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set());

export function toggleSelected(id: string) {
  setSelectedIds((prev) => {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  });
}

export function clearSelection() {
  setSelectedIds(new Set<string>());
}

export function isSelected(id: string): boolean {
  return selectedIds().has(id);
}

export function selectionCount(): number {
  return selectedIds().size;
}

export { selectedIds };
