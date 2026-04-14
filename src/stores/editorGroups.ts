import { createSignal } from "solid-js";
import type { SessionRef } from "../lib/types";

export interface EditorGroup {
  id: string;
  tabs: SessionRef[];
  activeTabId: string | null;
  flexBasis: number; // percentage, e.g. 100 = full width
}

const MAX_GROUPS = 4;
let nextGroupId = 1;

function makeGroup(tabs: SessionRef[] = [], flexBasis = 100): EditorGroup {
  return {
    id: String(nextGroupId++),
    tabs,
    activeTabId: tabs.length > 0 ? tabs[0].id : null,
    flexBasis,
  };
}

const [groups, setGroups] = createSignal<EditorGroup[]>([makeGroup()]);
const [activeGroupId, setActiveGroupId] = createSignal<string>(groups()[0].id);

// ---------- helpers ----------

function findGroupBySession(sessionId: string): EditorGroup | undefined {
  return groups().find((g) => g.tabs.some((t) => t.id === sessionId));
}

function activeGroup(): EditorGroup | undefined {
  return groups().find((g) => g.id === activeGroupId());
}

function updateGroup(groupId: string, fn: (g: EditorGroup) => EditorGroup) {
  setGroups((prev) => prev.map((g) => (g.id === groupId ? fn(g) : g)));
}

function removeGroupIfEmpty(groupId: string) {
  setGroups((prev) => {
    if (prev.length <= 1) return prev; // keep last group
    const g = prev.find((x) => x.id === groupId);
    if (g && g.tabs.length === 0) {
      const filtered = prev.filter((x) => x.id !== groupId);
      if (activeGroupId() === groupId) {
        setActiveGroupId(filtered[filtered.length - 1].id);
      }
      return filtered;
    }
    return prev;
  });
}

// ---------- actions ----------

function openSession(session: SessionRef) {
  const existing = findGroupBySession(session.id);
  if (existing) {
    setActiveGroupId(existing.id);
    updateGroup(existing.id, (g) => ({ ...g, activeTabId: session.id }));
    return;
  }
  const gId = activeGroupId();
  updateGroup(gId, (g) => ({
    ...g,
    tabs: [...g.tabs, session],
    activeTabId: session.id,
  }));
}

function closeTab(sessionId: string) {
  const g = findGroupBySession(sessionId);
  if (!g) return;
  const newTabs = g.tabs.filter((t) => t.id !== sessionId);
  const newActive =
    g.activeTabId === sessionId
      ? newTabs.length > 0
        ? newTabs[newTabs.length - 1].id
        : null
      : g.activeTabId;
  const gId = g.id;
  updateGroup(gId, (prev) => ({
    ...prev,
    tabs: newTabs,
    activeTabId: newActive,
  }));
  removeGroupIfEmpty(gId);
}

function closeAllTabs() {
  const g = makeGroup();
  setGroups([g]);
  setActiveGroupId(g.id);
}

function closeOtherTabs(keepId: string) {
  const g = findGroupBySession(keepId);
  if (!g) return;
  const kept = g.tabs.filter((t) => t.id === keepId);
  updateGroup(g.id, (prev) => ({ ...prev, tabs: kept, activeTabId: keepId }));
  // remove all other groups
  setGroups((prev) => prev.filter((x) => x.id === g.id));
  setActiveGroupId(g.id);
}

function closeTabsToRight(fromId: string) {
  const g = findGroupBySession(fromId);
  if (!g) return;
  const idx = g.tabs.findIndex((t) => t.id === fromId);
  if (idx === -1) return;
  const kept = g.tabs.slice(0, idx + 1);
  const newActive =
    g.activeTabId && kept.some((t) => t.id === g.activeTabId)
      ? g.activeTabId
      : fromId;
  updateGroup(g.id, (prev) => ({
    ...prev,
    tabs: kept,
    activeTabId: newActive,
  }));
}

function splitToRight(sessionId: string) {
  const sourceGroup = findGroupBySession(sessionId);
  if (!sourceGroup) return;
  // guard: sole tab in last group → no-op
  if (sourceGroup.tabs.length <= 1 && groups().length <= 1) return;

  const session = sourceGroup.tabs.find((t) => t.id === sessionId)!;
  // remove from source
  const newSourceTabs = sourceGroup.tabs.filter((t) => t.id !== sessionId);
  const newSourceActive =
    sourceGroup.activeTabId === sessionId
      ? newSourceTabs.length > 0
        ? newSourceTabs[newSourceTabs.length - 1].id
        : null
      : sourceGroup.activeTabId;

  const sourceIdx = groups().findIndex((g) => g.id === sourceGroup.id);
  const rightNeighbor = groups()[sourceIdx + 1];

  if (rightNeighbor) {
    // move to existing right group
    updateGroup(sourceGroup.id, (g) => ({
      ...g,
      tabs: newSourceTabs,
      activeTabId: newSourceActive,
    }));
    updateGroup(rightNeighbor.id, (g) => ({
      ...g,
      tabs: [...g.tabs, session],
      activeTabId: session.id,
    }));
    setActiveGroupId(rightNeighbor.id);
  } else if (groups().length < MAX_GROUPS) {
    // create new group, split source width 50/50
    const halfBasis = sourceGroup.flexBasis / 2;
    updateGroup(sourceGroup.id, (g) => ({
      ...g,
      tabs: newSourceTabs,
      activeTabId: newSourceActive,
      flexBasis: halfBasis,
    }));
    const newGroup = makeGroup([session], halfBasis);
    setGroups((prev) => {
      const result = [...prev];
      result.splice(sourceIdx + 1, 0, newGroup);
      return result;
    });
    setActiveGroupId(newGroup.id);
  } else {
    // at max groups, move to rightmost
    const rightmost = groups()[groups().length - 1];
    if (rightmost.id === sourceGroup.id) return; // already rightmost, no split target
    updateGroup(sourceGroup.id, (g) => ({
      ...g,
      tabs: newSourceTabs,
      activeTabId: newSourceActive,
    }));
    updateGroup(rightmost.id, (g) => ({
      ...g,
      tabs: [...g.tabs, session],
      activeTabId: session.id,
    }));
    setActiveGroupId(rightmost.id);
  }

  removeGroupIfEmpty(sourceGroup.id);
}

function moveTabToGroup(
  sessionId: string,
  targetGroupId: string,
  insertIndex?: number,
) {
  const sourceGroup = findGroupBySession(sessionId);
  if (!sourceGroup) return;
  if (sourceGroup.id === targetGroupId) {
    // reorder within group
    if (insertIndex === undefined) return;
    const tab = sourceGroup.tabs.find((t) => t.id === sessionId)!;
    const without = sourceGroup.tabs.filter((t) => t.id !== sessionId);
    without.splice(insertIndex, 0, tab);
    updateGroup(sourceGroup.id, (g) => ({ ...g, tabs: without }));
    return;
  }
  const session = sourceGroup.tabs.find((t) => t.id === sessionId)!;
  // remove from source
  const newSourceTabs = sourceGroup.tabs.filter((t) => t.id !== sessionId);
  const newSourceActive =
    sourceGroup.activeTabId === sessionId
      ? newSourceTabs.length > 0
        ? newSourceTabs[newSourceTabs.length - 1].id
        : null
      : sourceGroup.activeTabId;
  updateGroup(sourceGroup.id, (g) => ({
    ...g,
    tabs: newSourceTabs,
    activeTabId: newSourceActive,
  }));
  // add to target
  updateGroup(targetGroupId, (g) => {
    const tabs = [...g.tabs];
    if (insertIndex !== undefined) {
      tabs.splice(insertIndex, 0, session);
    } else {
      tabs.push(session);
    }
    return { ...g, tabs, activeTabId: session.id };
  });
  setActiveGroupId(targetGroupId);
  removeGroupIfEmpty(sourceGroup.id);
}

function createGroupFromDrop(sessionId: string): void {
  if (groups().length >= MAX_GROUPS) return;
  const sourceGroup = findGroupBySession(sessionId);
  if (!sourceGroup) return;
  const session = sourceGroup.tabs.find((t) => t.id === sessionId)!;

  const newSourceTabs = sourceGroup.tabs.filter((t) => t.id !== sessionId);
  const newSourceActive =
    sourceGroup.activeTabId === sessionId
      ? newSourceTabs.length > 0
        ? newSourceTabs[newSourceTabs.length - 1].id
        : null
      : sourceGroup.activeTabId;

  const halfBasis = sourceGroup.flexBasis / 2;
  updateGroup(sourceGroup.id, (g) => ({
    ...g,
    tabs: newSourceTabs,
    activeTabId: newSourceActive,
    flexBasis: halfBasis,
  }));

  const newGroup = makeGroup([session], halfBasis);
  setGroups((prev) => [...prev, newGroup]);
  setActiveGroupId(newGroup.id);
  removeGroupIfEmpty(sourceGroup.id);
}

function focusGroup(groupId: string) {
  if (groups().some((g) => g.id === groupId)) {
    setActiveGroupId(groupId);
  }
}

function focusAdjacentGroup(direction: "left" | "right") {
  const idx = groups().findIndex((g) => g.id === activeGroupId());
  if (idx === -1) return;
  const nextIdx = direction === "right" ? idx + 1 : idx - 1;
  const target = groups()[nextIdx];
  if (target) setActiveGroupId(target.id);
}

function setActiveTabInGroup(groupId: string, tabId: string) {
  updateGroup(groupId, (g) => ({ ...g, activeTabId: tabId }));
}

function setGroupFlexBasis(groupId: string, basis: number) {
  updateGroup(groupId, (g) => ({ ...g, flexBasis: basis }));
}

function syncAllTabTitles(titleMap: Map<string, string>) {
  setGroups((prev) => {
    let anyGroupChanged = false;
    const next = prev.map((g) => {
      let anyTabChanged = false;
      const newTabs = g.tabs.map((tab) => {
        const newTitle = titleMap.get(tab.id);
        if (newTitle && newTitle !== tab.title) {
          anyTabChanged = true;
          return { ...tab, title: newTitle };
        }
        return tab;
      });
      if (anyTabChanged) {
        anyGroupChanged = true;
        return { ...g, tabs: newTabs };
      }
      return g;
    });
    return anyGroupChanged ? next : prev;
  });
}

/** Reset store state — useful for testing */
function _reset() {
  nextGroupId = 1;
  const g = makeGroup();
  setGroups([g]);
  setActiveGroupId(g.id);
}

export {
  MAX_GROUPS,
  groups,
  activeGroupId,
  activeGroup,
  findGroupBySession,
  openSession,
  closeTab,
  closeAllTabs,
  closeOtherTabs,
  closeTabsToRight,
  splitToRight,
  moveTabToGroup,
  createGroupFromDrop,
  focusGroup,
  focusAdjacentGroup,
  setActiveGroupId,
  setActiveTabInGroup,
  setGroupFlexBasis,
  syncAllTabTitles,
  _reset,
};
