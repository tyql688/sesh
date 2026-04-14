import { describe, it, expect, beforeEach } from "vitest";
import type { SessionRef } from "../lib/types";
import {
  groups,
  activeGroupId,
  openSession,
  closeTab,
  closeAllTabs,
  closeOtherTabs,
  closeTabsToRight,
  splitToRight,
  moveTabToGroup,
  createGroupFromDrop,
  focusAdjacentGroup,
  _reset,
} from "./editorGroups";

function makeSession(id: string): SessionRef {
  return {
    id,
    provider: "claude",
    title: `Session ${id}`,
    project_name: "test",
    is_sidechain: false,
  };
}

describe("editorGroups store", () => {
  beforeEach(() => _reset());

  describe("openSession", () => {
    it("adds session to active group", () => {
      openSession(makeSession("s1"));
      expect(groups()[0].tabs).toHaveLength(1);
      expect(groups()[0].activeTabId).toBe("s1");
    });

    it("does not duplicate — activates existing", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s1"));
      expect(groups()[0].tabs).toHaveLength(2);
      expect(groups()[0].activeTabId).toBe("s1");
    });

    it("focuses group containing existing session", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s1"); // s1 moved to new group 2
      const g2Id = groups()[1].id;
      openSession(makeSession("s1"));
      expect(activeGroupId()).toBe(g2Id);
    });
  });

  describe("closeTab", () => {
    it("removes tab and activates previous", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      closeTab("s2");
      expect(groups()[0].tabs).toHaveLength(1);
      expect(groups()[0].activeTabId).toBe("s1");
    });

    it("auto-destroys empty non-last group", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      expect(groups()).toHaveLength(2);
      closeTab("s2");
      expect(groups()).toHaveLength(1);
    });

    it("keeps last group even when empty", () => {
      openSession(makeSession("s1"));
      closeTab("s1");
      expect(groups()).toHaveLength(1);
      expect(groups()[0].tabs).toHaveLength(0);
    });
  });

  describe("closeAllTabs", () => {
    it("resets to single empty group", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      closeAllTabs();
      expect(groups()).toHaveLength(1);
      expect(groups()[0].tabs).toHaveLength(0);
    });
  });

  describe("closeOtherTabs", () => {
    it("keeps only specified tab, collapses to one group", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s3"));
      splitToRight("s3");
      closeOtherTabs("s1");
      expect(groups()).toHaveLength(1);
      expect(groups()[0].tabs).toHaveLength(1);
      expect(groups()[0].tabs[0].id).toBe("s1");
    });
  });

  describe("closeTabsToRight", () => {
    it("removes tabs after the specified one in same group", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s3"));
      closeTabsToRight("s1");
      expect(groups()[0].tabs).toHaveLength(1);
    });
  });

  describe("splitToRight", () => {
    it("creates new group with the tab", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      expect(groups()).toHaveLength(2);
      expect(groups()[0].tabs.map((t) => t.id)).toEqual(["s1"]);
      expect(groups()[1].tabs.map((t) => t.id)).toEqual(["s2"]);
    });

    it("splits flexBasis 50/50 from source", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      expect(groups()[0].flexBasis).toBe(50);
      expect(groups()[1].flexBasis).toBe(50);
    });

    it("no-op when sole tab in only group", () => {
      openSession(makeSession("s1"));
      splitToRight("s1");
      expect(groups()).toHaveLength(1);
      expect(groups()[0].tabs).toHaveLength(1);
    });

    it("moves to existing right neighbor instead of creating", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s3"));
      splitToRight("s2"); // creates group 2 with s2
      splitToRight("s1"); // right neighbor exists (group with s2)
      expect(groups()).toHaveLength(2);
      expect(groups()[1].tabs.map((t) => t.id)).toContain("s1");
    });
  });

  describe("moveTabToGroup", () => {
    it("moves tab between groups", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      const g2Id = groups()[1].id;
      moveTabToGroup("s1", g2Id);
      expect(groups().find((g) => g.id === g2Id)!.tabs).toHaveLength(2);
    });

    it("reorders within same group", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s3"));
      const gId = groups()[0].id;
      moveTabToGroup("s3", gId, 0);
      expect(groups()[0].tabs.map((t) => t.id)).toEqual(["s3", "s1", "s2"]);
    });
  });

  describe("createGroupFromDrop", () => {
    it("creates new group at end", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      createGroupFromDrop("s2");
      expect(groups()).toHaveLength(2);
      expect(groups()[1].tabs[0].id).toBe("s2");
    });

    it("respects max group limit", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      openSession(makeSession("s3"));
      openSession(makeSession("s4"));
      openSession(makeSession("s5"));
      // build up to 4 groups via createGroupFromDrop
      createGroupFromDrop("s2"); // 2 groups
      createGroupFromDrop("s3"); // 3 groups
      createGroupFromDrop("s4"); // 4 groups
      createGroupFromDrop("s5"); // should be no-op (at MAX_GROUPS)
      expect(groups()).toHaveLength(4);
    });
  });

  describe("focusAdjacentGroup", () => {
    it("moves focus right", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      const g1Id = groups()[0].id;
      focusAdjacentGroup("left");
      expect(activeGroupId()).toBe(g1Id);
      focusAdjacentGroup("right");
      expect(activeGroupId()).toBe(groups()[1].id);
    });

    it("no-op at boundary", () => {
      openSession(makeSession("s1"));
      const gId = groups()[0].id;
      focusAdjacentGroup("left");
      expect(activeGroupId()).toBe(gId);
    });
  });

  describe("session uniqueness", () => {
    it("session exists in only one group at a time", () => {
      openSession(makeSession("s1"));
      openSession(makeSession("s2"));
      splitToRight("s2");
      const matches = groups().filter((g) => g.tabs.some((t) => t.id === "s2"));
      expect(matches).toHaveLength(1);
    });
  });
});
