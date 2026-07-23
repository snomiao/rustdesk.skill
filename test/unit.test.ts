// Unit tests for rustdesk-skill. Mock-free where possible (pure functions);
// the @playwright/cli boundary is stubbed via RDS_PW_BIN=true for the few impure paths.
//
// PUBLIC repo: fixtures use placeholder ids/passwords only — no real secrets.

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  NoPasswordError,
  getPassword,
  loadPasswords,
  resolvePassword,
  setPassword,
} from "../src/passwords";
import {
  boxCenter,
  langArg,
  matchText,
  normalizeBoxes,
  resolveBackend,
  type Box,
} from "../src/ocr";
import {
  parseKeyChord,
  normalizeKeyName,
  parseEvalResult,
  pw,
  PwNotFoundError,
  screenshotToCss,
} from "../src/pw";
import { sanitizeSessionName, defaultSessionName, resolveSessionName } from "../src/session";
import { needsShiftWarning, shiftChars, type, ShiftMangleError } from "../src/screen";
import { pickPasswordField, homeScreenPresent } from "../src/connect";
import { buildParser, type CliDeps } from "../bin/rustdesk-skill";

// --- password store ---

describe("password store", () => {
  let dir: string;
  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "rds-pw-"));
    process.env.RDS_CONFIG_DIR = dir;
  });
  afterEach(() => {
    delete process.env.RDS_CONFIG_DIR;
    rmSync(dir, { recursive: true, force: true });
  });

  test("set/get roundtrip", () => {
    setPassword("123456789", "secret-placeholder");
    expect(getPassword("123456789")).toBe("secret-placeholder");
  });

  test("file is created with mode 600", () => {
    setPassword("123456789", "pw");
    const mode = statSync(join(dir, "passwords.json")).mode & 0o777;
    expect(mode).toBe(0o600);
  });

  test("missing store reads as empty", () => {
    expect(loadPasswords()).toEqual({});
    expect(getPassword("nope")).toBeUndefined();
  });
});

describe("resolvePassword precedence", () => {
  test("explicit arg wins", () => {
    expect(resolvePassword("123456789", "explicit", { "123456789": "remembered" })).toBe("explicit");
  });
  test("falls back to remembered", () => {
    expect(resolvePassword("123456789", undefined, { "123456789": "remembered" })).toBe("remembered");
  });
  test("empty arg falls back to remembered", () => {
    expect(resolvePassword("123456789", "", { "123456789": "remembered" })).toBe("remembered");
  });
  test("throws NoPasswordError when none", () => {
    expect(() => resolvePassword("123456789", undefined, {})).toThrow(NoPasswordError);
  });
});

// --- OCR normalization + matching ---

describe("normalizeBoxes", () => {
  test("paddle pixel boxes pass through", () => {
    const boxes = normalizeBoxes([{ text: "OK", confidence: 0.9, x1: 10, y1: 20, x2: 50, y2: 40 }], 1000, 800);
    expect(boxes[0]).toEqual({ text: "OK", conf: 0.9, x: 10, y: 20, w: 40, h: 20 });
  });
  test("vision normalized boxes scale to png px", () => {
    const boxes = normalizeBoxes(
      [{ text: "Connect", confidence: 0.8, x1_norm: 0.5, y1_norm: 0.25, x2_norm: 0.6, y2_norm: 0.3 }],
      1000,
      800,
    );
    // x: 0.5*1000=500, y:0.25*800=200, w:0.1*1000=100, h:0.05*800=40
    expect(boxes[0]).toEqual({ text: "Connect", conf: 0.8, x: 500, y: 200, w: 100, h: 40 });
  });
});

describe("matchText", () => {
  const boxes: Box[] = [
    { text: "OK", conf: 1, x: 0, y: 0, w: 20, h: 10 },
    { text: "Cancel", conf: 1, x: 30, y: 0, w: 40, h: 10 },
    { text: "OK", conf: 1, x: 0, y: 50, w: 20, h: 10 },
  ];
  test("exact, case-insensitive", () => {
    const r = matchText(boxes, "ok");
    expect(r.box?.y).toBe(0);
    expect(r.ambiguous).toBe(true); // two OKs
  });
  test("nth disambiguates", () => {
    expect(matchText(boxes, "OK", { nth: 2 }).box?.y).toBe(50);
  });
  test("contains matches substring", () => {
    expect(matchText(boxes, "anc", { contains: true }).box?.text).toBe("Cancel");
  });
  test("no match returns undefined box", () => {
    expect(matchText(boxes, "nope").box).toBeUndefined();
  });
  test("boxCenter is the middle", () => {
    expect(boxCenter({ text: "x", conf: 1, x: 10, y: 20, w: 40, h: 20 })).toEqual({ x: 30, y: 30 });
  });
});

describe("OCR backend resolution", () => {
  afterEach(() => delete process.env.RDS_OCR);
  test("flag beats env beats default", () => {
    process.env.RDS_OCR = "paddle";
    expect(resolveBackend("vision")).toBe("vision");
    expect(resolveBackend()).toBe("paddle");
  });
  test("lang mapping", () => {
    expect(langArg("vision", "ja,en")).toBe("ja,en-US");
    expect(langArg("paddle", "ja,en")).toBe("japan");
  });
});

// --- coordinate conversion (the invariant) ---

describe("screenshotToCss", () => {
  test("DPR=2 retina halves coords", () => {
    // png 2000 wide, viewport 1000 css → scale 0.5
    expect(screenshotToCss(1000, 600, 2000, 1200, 1000, 600)).toEqual({ x: 500, y: 300 });
  });
  test("DPR=1 is identity", () => {
    expect(screenshotToCss(640, 480, 1280, 1024, 1280, 1024)).toEqual({ x: 640, y: 480 });
  });
  test("unknown viewport assumes px==css", () => {
    expect(screenshotToCss(100, 100, 0, 0, 0, 0)).toEqual({ x: 100, y: 100 });
  });
});

// --- @playwright/cli eval result unwrapping ---

describe("parseEvalResult", () => {
  test("unwraps a JSON string result", () => {
    const out = `### Result\n"https://rustdesk.com/web/"\n### Ran Playwright code\n\`\`\`js\nawait page.evaluate('...');\n\`\`\``;
    expect(parseEvalResult(out)).toBe('"https://rustdesk.com/web/"');
  });
  test("unwraps a pretty-printed JSON object result", () => {
    const out = `### Result\n{\n  "w": 1280,\n  "h": 720\n}\n### Ran Playwright code`;
    expect(JSON.parse(parseEvalResult(out))).toEqual({ w: 1280, h: 720 });
  });
  test("falls back to trimmed output when no Result header", () => {
    expect(parseEvalResult("  raw value  ")).toBe("raw value");
  });
});

// --- key chord parsing ---

describe("parseKeyChord", () => {
  test("plain key", () => {
    expect(parseKeyChord("Enter")).toEqual({ key: "Enter", mods: [] });
  });
  test("ctrl+c", () => {
    expect(parseKeyChord("ctrl+c")).toEqual({ key: "c", mods: ["Control"] });
  });
  test("alt+Tab maps friendly names", () => {
    expect(parseKeyChord("alt+tab")).toEqual({ key: "Tab", mods: ["Alt"] });
  });
  test("unknown modifier throws", () => {
    expect(() => parseKeyChord("hyper+x")).toThrow();
  });
  test("normalizeKeyName", () => {
    expect(normalizeKeyName("esc")).toBe("Escape");
    expect(normalizeKeyName("down")).toBe("ArrowDown");
  });
});

// --- password dialog: pick the input field (from real OCR layout) ---

describe("pickPasswordField", () => {
  const title: Box = { text: "※** パスワードが必要です", conf: 1, x: 524, y: 250, w: 228, h: 20 };
  const boxes: Box[] = [
    title,
    { text: "RustDesk のパスワードを確認する", conf: 1, x: 514, y: 302, w: 248, h: 18 },
    { text: "白 パスワード", conf: 0.5, x: 526, y: 350, w: 116, h: 22 }, // the input
    { text: "パスワードを記憶する", conf: 1, x: 566, y: 406, w: 132, h: 14 },
    { text: "✕ キャンセル", conf: 1, x: 546, y: 458, w: 112, h: 22 },
    { text: "<OK", conf: 0.5, x: 690, y: 460, w: 62, h: 18 },
  ];
  test("picks the input, not the title/subtitle/remember", () => {
    const f = pickPasswordField(boxes, title);
    expect(f?.y).toBe(350);
    expect(f?.text).toBe("白 パスワード");
  });
  test("skips password-mentioning boxes above the title", () => {
    const withAbove = [{ text: "パスワード", conf: 1, x: 0, y: 100, w: 10, h: 10 }, ...boxes];
    expect(pickPasswordField(withAbove, title)?.y).toBe(350);
  });
});

// --- home vs connected detection (from real OCR) ---

describe("homeScreenPresent", () => {
  test("true on the ID-entry home (リモートID field present)", () => {
    const home: Box[] = [
      { text: "RustDesk （Preview）", conf: 1, x: 556, y: 20, w: 168, h: 22 },
      { text: "リモートデスクトップを操作@", conf: 1, x: 347, y: 69, w: 277, h: 27 },
      { text: "リモートID", conf: 1, x: 358, y: 135, w: 84, h: 18 },
    ];
    expect(homeScreenPresent(home)).toBe(true);
  });
  test("false while viewing a remote (no home UI markers)", () => {
    const remote: Box[] = [
      { text: "組織ダッシュボード（管理者）", conf: 1, x: 357, y: 389, w: 229, h: 27 },
      { text: "新しいタブ", conf: 1, x: 370, y: 96, w: 76, h: 20 },
    ];
    expect(homeScreenPresent(remote)).toBe(false);
  });
});

// --- shift-mangling guard ---

describe("type shift-warning", () => {
  test("detects shift chars", () => {
    expect(shiftChars("Hello@world+")).toEqual(["H", "@", "+"]);
    expect(needsShiftWarning("hello-123.4/5")).toBe(false);
    expect(needsShiftWarning("Password!")).toBe(true);
  });

  test("type refuses shift chars without --force", () => {
    // typeText is never reached on the refusal path, so no pw spawn happens.
    expect(() => type("Secret@1")).toThrow(ShiftMangleError);
  });

  test("type allows lowercase/digits (pw stubbed)", () => {
    process.env.RDS_PW_BIN = "true"; // /usr/bin/true — succeeds, no-op
    process.env.RDS_SESSION_NAME = "test-session";
    try {
      expect(() => type("hello-123")).not.toThrow();
      expect(() => type("Secret@1", { force: true })).not.toThrow();
    } finally {
      delete process.env.RDS_PW_BIN;
      delete process.env.RDS_SESSION_NAME;
    }
  });
});

// --- missing @playwright/cli → clear actionable error (no raw stack trace) ---

describe("pw backend availability", () => {
  test("throws PwNotFoundError when the binary is missing", () => {
    process.env.RDS_PW_BIN = "rds-definitely-not-a-real-binary-xyz";
    process.env.RDS_SESSION_NAME = "test-session";
    try {
      expect(() => pw(["status"])).toThrow(PwNotFoundError);
      expect(() => pw(["status"])).toThrow(/not found on PATH/);
    } finally {
      delete process.env.RDS_PW_BIN;
      delete process.env.RDS_SESSION_NAME;
    }
  });
});

// --- session name resolution ---

describe("session name", () => {
  test("sanitize keeps safe chars, collapses the rest", () => {
    expect(sanitizeSessionName("my repo/feature #1")).toBe("my-repo-feature-1");
    expect(sanitizeSessionName("  --Foo.Bar_baz--  ")).toBe("Foo.Bar_baz");
  });

  test("env RDS_SESSION_NAME wins and is sanitized", () => {
    process.env.RDS_SESSION_NAME = "My Proj/Main";
    try {
      expect(resolveSessionName()).toBe("My-Proj-Main");
    } finally {
      delete process.env.RDS_SESSION_NAME;
    }
  });

  test("default is derived from git/cwd (non-empty in this repo)", () => {
    expect(defaultSessionName().length).toBeGreaterThan(0);
  });

  test("non-interactive resolve falls back to a default (no throw) when unset", () => {
    delete process.env.RDS_SESSION_NAME;
    // bun test runs non-interactively, so this returns the derived default.
    expect(resolveSessionName().length).toBeGreaterThan(0);
  });
});

// --- yargs parsing / dispatch (deps injected) ---

describe("CLI parsing + dispatch", () => {
  function spyDeps(): { deps: CliDeps; calls: Record<string, unknown[]> } {
    const calls: Record<string, unknown[]> = {};
    const rec = (name: string) => (...args: unknown[]) => {
      calls[name] = args;
      return undefined as never;
    };
    const deps: CliDeps = {
      connect: ((o: unknown) => {
        calls.connect = [o];
        return { ok: true, id: "x", connected: false };
      }) as CliDeps["connect"],
      status: rec("status") as CliDeps["status"],
      disconnect: rec("disconnect") as CliDeps["disconnect"],
      listRecent: (() => {
        calls.listRecent = [];
        return [];
      }) as CliDeps["listRecent"],
      screenshot: ((f?: string) => {
        calls.screenshot = [f];
        return "out.png";
      }) as CliDeps["screenshot"],
      ocr: (() => {
        calls.ocr = [];
        return [];
      }) as CliDeps["ocr"],
      clickText: ((t: string, o: unknown) => {
        calls.clickText = [t, o];
        return { text: t, conf: 1, x: 0, y: 0, w: 1, h: 1 };
      }) as CliDeps["clickText"],
      clickXY: rec("clickXY") as CliDeps["clickXY"],
      move: rec("move") as CliDeps["move"],
      scroll: rec("scroll") as CliDeps["scroll"],
      type: rec("type") as CliDeps["type"],
      key: rec("key") as CliDeps["key"],
      paste: rec("paste") as CliDeps["paste"],
    };
    return { deps, calls };
  }

  test("connect alias 'c' with pw + --remember-password", () => {
    const { deps, calls } = spyDeps();
    buildParser(["c", "123456789", "placeholder-pw", "--remember-password"], deps).parse();
    expect(calls.connect[0]).toMatchObject({
      id: "123456789",
      pw: "placeholder-pw",
      rememberPassword: true,
      fresh: false,
    });
  });

  test("connect without pw leaves pw undefined", () => {
    const { deps, calls } = spyDeps();
    buildParser(["connect", "123456789"], deps).parse();
    expect((calls.connect[0] as { pw?: string }).pw).toBeUndefined();
  });

  test("click-xy parses numbers", () => {
    const { deps, calls } = spyDeps();
    buildParser(["click-xy", "685", "315"], deps).parse();
    expect(calls.clickXY).toEqual([685, 315]);
  });

  test("type --force passes force flag", () => {
    const { deps, calls } = spyDeps();
    buildParser(["type", "Hello@", "--force"], deps).parse();
    expect(calls.type).toEqual(["Hello@", { force: true }]);
  });

  test("click --nth --contains", () => {
    const { deps, calls } = spyDeps();
    buildParser(["click", "OK", "--nth", "2", "--contains"], deps).parse();
    expect(calls.clickText[0]).toBe("OK");
    expect(calls.clickText[1]).toMatchObject({ nth: 2, contains: true });
  });

  test("ss alias maps to screenshot", () => {
    const { deps, calls } = spyDeps();
    buildParser(["ss", "shot.png"], deps).parse();
    expect(calls.screenshot).toEqual(["shot.png"]);
  });
});
