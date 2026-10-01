/**
 * Settings › Capabilities › Mobile Development, as facts (ide/19): the
 * keys, the platforms this machine develops for and what each choice means,
 * who may drive the devices, the status card's sentence, one row per
 * component of the toolchain with what was found and the official way to
 * install what was not, Flutter's own doctor lines, the devices with their
 * verbs, and what a new simulator needs. The registry panel draws the two
 * paths. Plain `.mjs`, so `node --test` reads it.
 */

import { t as tr } from "../../i18n/l10n.mjs";

export const ENABLED_KEY = "mobile_development.enabled";
export const PLATFORMS_KEY = "mobile_development.platforms";
export const AGENTS_KEY = "mobile_development.agents";

/** The platforms, in the order the switch shows. */
export const PLATFORMS = Object.freeze(["both", "ios", "android"]);
export const DEFAULT_PLATFORMS = "both";
/** Who may drive the devices through the tools, in the order the switch shows. */
export const POLICIES = Object.freeze(["everyone", "assigned", "nobody"]);
export const DEFAULT_POLICY = "everyone";

/** The reason iOS is held off macOS. */
const IOS_NEEDS_MAC = tr("settings-mobile-development-settings-ios-needs-xcode-which-runs-macos");

/**
 * The platforms switch's segments: iOS held with the reason on a machine
 * that is not a Mac.
 * @param {boolean} mac
 */
export function platformSegments(mac) {
  return [
    { id: "both", label: tr("settings-mobile-development-settings-ios-android"), icon: "device", disabled: !mac, hint: mac ? tr("settings-mobile-development-settings-simulators-iphones-emulators-android-phones") : IOS_NEEDS_MAC },
    { id: "ios", label: "iOS", icon: "simulator", disabled: !mac, hint: mac ? tr("settings-mobile-development-settings-simulators-iphones-alone") : IOS_NEEDS_MAC },
    { id: "android", label: tr("settings-mobile-development-settings-android"), icon: "device", disabled: false, hint: tr("settings-mobile-development-settings-emulators-android-phones-alone") },
  ];
}

/** What the switch shows: the value, or Android where iOS cannot be. @param {string} value @param {boolean} mac */
export function platformShown(value, mac) {
  return !mac && value !== "android" ? "android" : value;
}

/** The platforms as a sentence says them. @param {string} platforms */
export function platformWords(platforms) {
  switch (platforms) {
    case "ios":
      return tr("settings-mobile-development-settings-ios-only");
    case "android":
      return tr("settings-mobile-development-settings-android-only");
    default:
      return tr("settings-mobile-development-settings-ios-android");
  }
}

/** The policy switch's segments. */
export function policySegments() {
  return [
    { id: "everyone", label: tr("settings-browser-settings-everyone"), icon: "agent" },
    { id: "assigned", label: tr("settings-browser-settings-assigned"), icon: "skill" },
    { id: "nobody", label: tr("settings-browser-settings-nobody"), icon: "close" },
  ];
}

/** What a policy means, in a sentence under the switch. @param {string} policy */
export function policyWords(policy) {
  switch (policy) {
    case "assigned":
      return tr("settings-mobile-development-settings-only-agents-carry-flutter-development-skill");
    case "nobody":
      return tr("settings-mobile-development-settings-mobile-tools-refuse-every-agent-platform");
    default:
      return tr("settings-mobile-development-settings-any-agent-may-list-devices-boot");
  }
}

/**
 * The status card: whether mobile development is on here, for which
 * platforms, and whether Flutter is there to develop with.
 * @param {{enabled: boolean, platforms: string, flutter: boolean | null}} facts `flutter` null while the toolchain is unread
 * @returns {{tone: "ok" | "warn" | "quiet", label: string, sentence: string}}
 */
export function statusWords({ enabled, platforms, flutter }) {
  if (!enabled) {
    return { tone: "quiet", label: tr("settings-mobile-development-settings-off"), sentence: tr("settings-mobile-development-settings-mobile-development-off-machine-device-listed") };
  }
  if (flutter === false) {
    return { tone: "warn", label: tr("settings-mobile-development-settings-flutter"), sentence: tr("settings-mobile-development-settings-machine-develops-but-flutter-found-install", { platforms: platformWords(platforms) }) };
  }
  return { tone: "ok", label: tr("settings-mobile-development-settings-words"), sentence: tr("settings-mobile-development-settings-machine-develops-flutter-apps-devices-below", { platforms: platformWords(platforms) }) };
}

/** The official way in, per component: where to read, and the command the person runs. */
export const INSTALL = Object.freeze({
  flutter: { url: "https://docs.flutter.dev/get-started/install", text: tr("settings-mobile-development-settings-install-flutter"), command: "flutter doctor" },
  xcode: { url: "https://apps.apple.com/app/xcode/id497799835", text: tr("settings-mobile-development-settings-install-xcode"), command: "xcode-select --install && sudo xcodebuild -runFirstLaunch" },
  runtime: { url: "https://developer.apple.com/documentation/xcode/installing-additional-simulator-runtimes", text: tr("settings-mobile-development-settings-install-runtime"), command: "xcodebuild -downloadPlatform iOS" },
  cocoapods: { url: "https://guides.cocoapods.org/using/getting-started.html", text: tr("settings-mobile-development-settings-install-cocoapods"), command: "sudo gem install cocoapods" },
  android: { url: "https://developer.android.com/studio", text: tr("settings-mobile-development-settings-install-android"), command: "flutter doctor --android-licenses" },
  avd: { url: "https://developer.android.com/studio/run/managing-avds", text: tr("settings-mobile-development-settings-install-avd"), command: null },
  java: { url: "https://developer.android.com/build/jdks", text: tr("settings-mobile-development-settings-install-java"), command: null },
});

/**
 * One row per component, in the order a person installs them: Flutter,
 * then the iOS side, then the Android side. A side that is off, or iOS off
 * macOS, is held with the reason rather than shown as missing.
 * @param {import("../../types").MobileToolchain | null} toolchain
 * @param {{platforms: string, mac: boolean}} facts
 * @returns {{id: string, name: string, tone: "ok" | "warn" | "quiet", found: string, install: {url: string, text: string, command: string | null} | null, held: string | null}[]}
 */
export function componentRows(toolchain, { platforms, mac }) {
  const t = toolchain;
  const row = (id, name, ok, found, install, held = null) => ({
    id,
    name,
    tone: held ? "quiet" : ok ? "ok" : "warn",
    found: held ? held : found,
    install: !held && !ok ? install : null,
    held,
  });
  const iosHeld = !mac ? IOS_NEEDS_MAC : platforms === "android" ? tr("settings-mobile-development-settings-ios-off-platforms-above") : null;
  const androidHeld = platforms === "ios" ? tr("settings-mobile-development-settings-android-off-platforms-above") : null;
  if (!t) return [];
  const flutter = t.flutter;
  const runtimes = (t.ios_runtimes ?? []).filter((r) => r.available);
  const avds = t.android?.avds ?? [];
  return [
    row("flutter", "Flutter", flutter.installed, flutter.installed ? tr("settings-mobile-development-settings-flutter-found", { version: flutter.version ?? tr("settings-mobile-development-settings-installed"), channel: flutter.channel ?? "", channel_flag: flutter.channel ? "yes" : "no", dart: flutter.dart ?? "", dart_flag: flutter.dart ? "yes" : "no", path: flutter.path ?? "" }) : tr("settings-mobile-development-settings-found-machine"), INSTALL.flutter),
    row("xcode", "Xcode", t.xcode.installed, t.xcode.installed ? tr("settings-mobile-development-settings-xcode", { version: t.xcode.version ?? "", xcode: t.xcode.path, flag: (t.xcode.path) ? "yes" : "no" }) : tr("settings-mobile-development-settings-found-xcodebuild-answered-nothing"), INSTALL.xcode, iosHeld),
    row("runtime", tr("settings-mobile-development-settings-ios-simulator-runtime"), runtimes.length > 0, runtimes.length > 0 ? runtimes.map((r) => r.name).join(", ") : tr("settings-mobile-development-settings-ios-runtime-downloaded"), INSTALL.runtime, iosHeld),
    row("cocoapods", "CocoaPods", t.cocoapods.installed, t.cocoapods.installed ? tr("settings-mobile-development-settings-cocoapods", { version: t.cocoapods.version ?? "" }) : tr("settings-mobile-development-settings-found"), INSTALL.cocoapods, iosHeld),
    row("android", tr("settings-mobile-development-settings-android-sdk"), Boolean(t.android?.path && t.android?.adb), t.android?.path ? tr("settings-mobile-development-settings-android-sdk-found", { path: t.android.path, adb: t.android.adb ? "yes" : "no", emulator: t.android.emulator ? "yes" : "no" }) : tr("settings-mobile-development-settings-found-neither-android-home-nor-android"), INSTALL.android, androidHeld),
    row("avd", tr("settings-mobile-development-settings-android-virtual-device"), avds.length > 0, avds.length > 0 ? avds.join(", ") : tr("settings-mobile-development-settings-virtual-device-made"), INSTALL.avd, androidHeld),
    row("java", "Java", t.java.installed, t.java.installed ? tr("settings-mobile-development-settings-java", { version: t.java.version ?? "" }) : tr("settings-mobile-development-settings-found"), INSTALL.java, androidHeld),
  ];
}

/** Flutter's own lines, with a tone each. @param {readonly {state: string, name: string, detail?: string | null}[]} doctor */
export function doctorRows(doctor) {
  return (doctor ?? []).map((d) => ({
    tone: d.state === "ok" ? "ok" : d.state === "missing" ? "warn" : "neutral",
    name: d.name,
    detail: d.detail ?? null,
  }));
}

/** When the machine was last examined, as words. @param {number} checkedAt unix seconds @param {number} now */
export function checkedWords(checkedAt, now = Date.now() / 1000) {
  if (!checkedAt) return tr("settings-mcp-health-checked-yet");
  const ago = Math.max(0, Math.round(now - checkedAt));
  if (ago < 60) return tr("settings-mcp-health-checked-just-now");
  if (ago < 3600) return tr("settings-mobile-development-settings-checked-m-ago", { ago: Math.round(ago / 60) });
  if (ago < 86400) return tr("settings-mobile-development-settings-checked-h-ago", { ago: Math.round(ago / 3600) });
  return tr("settings-mobile-development-settings-checked-d-ago", { ago: Math.round(ago / 86400) });
}

/**
 * The devices' rows: each with its words and the verbs its kind and state
 * allow — boot a shut-down simulator or emulator; shut a booted one down and,
 * for a simulator, show its window; a phone is plugged in and unlocked, never
 * booted from here.
 * @param {readonly import("../../types").MobileDevice[]} devices
 * @param {string} platforms
 * @returns {{id: string, name: string, words: string, up: boolean, verbs: ("boot" | "shutdown" | "show")[]}[]}
 */
export function deviceRows(devices, platforms) {
  return devices
    .filter((d) => platforms === "both" || platforms === d.platform)
    .map((d) => {
      const up = d.state === "booted" || d.state === "running";
      const verbs = [];
      if (d.kind !== "physical") {
        if (!up) verbs.push("boot");
        else {
          verbs.push("shutdown");
          if (d.platform === "ios") verbs.push("show");
        }
      }
      const kind = d.kind === "physical" ? "device" : d.kind;
      const state = d.state === "shutdown" ? tr("settings-mobile-development-settings-shut-down") : d.state;
      return { id: d.id, name: d.name, words: `${d.os ? `${d.os} · ` : ""}${kind} · ${state}`, up, verbs };
    });
}

/** What keeps a new simulator from being made, in a sentence, or `null`. @param {{name: string, devicetype: string, runtime: string}} draft */
export function simulatorProblem({ name, devicetype, runtime }) {
  if (!String(name ?? "").trim()) return tr("settings-mobile-development-settings-name-simulator");
  if (!devicetype) return tr("settings-mobile-development-settings-choose-device-type");
  if (!runtime) return tr("settings-mobile-development-settings-choose-runtime");
  return null;
}
