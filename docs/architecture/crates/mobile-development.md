# bisa-mobile-development

The mobile toolchain and the devices on this machine, as typed, time-boxed, argv-only subprocess
calls — `flutter`, `xcrun simctl`, `xcodebuild`, `adb`, `emulator`, `pod`, `java` — a leaf crate
like `bisa-ssh`, with no dependency on anything of ours
([ide/19](../ide/19-mobile-development.md)). It says what is installed for Flutter development on
iOS and Android and what Flutter's own doctor says, lists the simulators, emulators and phones this
machine can reach, boots and shuts a simulator or an emulator down, brings the Simulator window
forward, makes a simulator, and captures a device's screen as PNG or JPEG bytes. It composes the
one `flutter run` line a terminal runs and never runs it: the app runs where a person watches it.
The process launcher is a port with a fake, and no test of any crate spawns a mobile program.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the types — `Platform { Ios, Android }`, `DeviceKind { Simulator, Emulator, Physical }`, `DeviceState { Booted, Shutdown, Running, Offline }` (`is_up`), `ImageFormat { Png, Jpeg }`, `Device { id, name, platform, kind, state, os? }` (`words`), `Toolchain { flutter: FlutterInfo, xcode: XcodeInfo, ios_runtimes, ios_devicetypes, cocoapods: ToolInfo, android: AndroidSdk, java: ToolInfo, doctor: Vec<DoctorLine> }`, `Hints { flutter_path?, android_sdk? }` (the two settings, passed per call), `MobileDevelopmentError { NotInstalled, Timeout, Failed, NoSuchDevice, Unsupported }` — and the port `MobileDevelopmentTools: Send + Sync + Debug` (`toolchain`, `flutter_path`, `devices`, `boot`, `shutdown`, `show`, `create_simulator`, `screenshot`), synchronous, driven from `spawn_blocking`; `run_command(flutter, device)` (the line, `shlex`-quoted), `png_size(bytes)` (the IHDR) |
| `exec.rs` | the one process launcher: `Exec { timeouts: Timeouts { local 15 s, doctor 90 s, boot 120 s } }`, `run(program: &Path, args, env, timeout) -> Output { code, stdout: Vec<u8>, stderr }` — by absolute path, argv only, stdin null, `HARDENED_ENV` (`NO_COLOR`, `TERM=dumb`, `LC_ALL=C`, `PAGER=cat`, `FLUTTER_SUPPRESS_ANALYTICS`, `CI`) over the process's own, the child **terminated** on expiry, `NotFound` read as `NotInstalled`, a pipe that could not be read to its end `Failed` rather than a truncated frame taken whole; `spawn_detached(program, args, env)` for a program that keeps a window — every stream null, never waited on |
| `resolve.rs` | where the programs are, pure over injected probes: `flutter_path(hint, places, on_path)` — the setting (a file, or a folder taken as the Flutter home), then `PATH`, then the `Places`: `FLUTTER_HOMES` under its home, then its system folders (`Places::machine` is the home and `FLUTTER_SYSTEM`, Homebrew's; `Places::under(home)` has none, so a test reads nothing of the machine); `android_sdk(hint, env, home)` — the setting, `ANDROID_HOME` then `ANDROID_SDK_ROOT` read by name, then Android Studio's `Library/Android/sdk`; `adb_of`, `emulator_of` under the SDK |
| `parse.rs` | each program's output read once into a typed value, on fixtures: `flutter_version` (`--version --machine`, after any notice), `doctor_lines` (`[✓]`/`[✗]`/`[!]` and the Windows marks, the name before the dash, the detail in the parentheses), `simctl_devices` (iOS runtimes alone, the OS from the runtime's id, unavailable ones dropped), `simctl_runtimes`, `simctl_devicetypes` (iPhone and iPad), `adb_devices` (`-l`: `emulator-NNNN` an emulator, else a phone; `model:` the name; `offline` and `unauthorized` both offline), `avd_list`, `avd_name`, `xcodebuild_version`, `xcode_select`, `pod_version`, `java_version` (stderr) |
| `real.rs` | `Real { exec, home }` (`Default` from the home directory) — the port over the real programs: `toolchain` probes every part on its own and never errors; `devices` is the available simulators, `adb`'s list with each running emulator named after its image, and every image nobody started as a shut-down emulator, the ones that are up first; `boot` is `simctl boot` + `bootstatus -b` (exit 149 is *already booted*), or the emulator started detached and looked for every two seconds until `adb` sees it; `shutdown` is `simctl shutdown` / `adb emu kill`; `show` is `open -a Simulator` detached; `create_simulator` is `simctl create`, the UDID answered; `screenshot` is `simctl io <udid> screenshot --type=<fmt>` into a `NamedTempFile` read and dropped, or `adb exec-out screencap -p` (PNG whatever was asked); a phone is not booted, shut down or shown from here (`Unsupported`) |
| `fake.rs` | `FakeMobileDevelopment` — the scripted port every test hands in: `with_toolchain`, `with_devices`, `with_screenshot`, `failing(op, error)`, `calls()`; a boot moves the device to booted or running, a shutdown back, a made simulator joins the list; `png_fixture(width, height)`, `simulator(...)`, `emulator(...)` for a test's rows |

---

A frame or a screenshot is bounded by `bisa_engine::mobile_development::MAX_FRAME_BYTES` (32 MiB): a tool that
answers more is refused with the size, and nothing that size reaches the desktop or an attachment.
An iOS simulator that is not up has nothing on its screen, as an Android device has not: both are
refused in words before any tool is asked.

## Entry points

`Real::default()` is what `bisa-cli` hands `EngineConfig.mobile_development` (`ctx.rs`); a test hands in a
`FakeMobileDevelopment`. The engine holds it in `EngineConfig.mobile_development` — `None` by default, so a fixture that
says nothing about mobile spawns nothing, every mobile route answers **503**, and the tools answer
*not available*. The engine's `mobile_development.rs` composes the workspace's word (`mobile_development.*`) over this
crate: the platforms' filter, the policy, the toolchain's TTL, the stored capture, the run line.

---

## Invariants held here

| Invariant | Held by |
|---|---|
| A program that is not there is *not installed*, for a run and for a detached spawn alike; an `Output` prints its size and never its bytes | `exec.rs::a_program_that_is_not_there_is_said_to_be_not_installed`, `an_output_says_its_size_and_not_its_bytes` |
| The run line quotes a path with spaces and names the device; a PNG's size is read off its IHDR and anything else is refused; a device says itself in one line | `lib.rs::run_command_quotes_a_path_with_spaces_and_the_device_id`, `png_size_reads_the_ihdr_and_refuses_anything_else`, `a_device_says_itself_in_one_line` |
| Each program's output is read once into a typed value: Flutter's version after any notice, the doctor's category lines, the iOS simulators with their OS, the iOS runtimes and device types, `adb`'s emulators, phones and offline ones, the small programs' answers | `parse.rs::flutters_version_is_read_off_the_machine_object_even_after_a_notice`, `the_doctors_category_lines_are_read_and_its_detail_lines_left`, `simulators_are_read_with_their_os_and_the_other_platforms_left`, `runtimes_and_device_types_are_read_for_ios_alone`, `adb_lists_emulators_phones_and_the_offline_ones`, `the_small_programs_answers_are_read` |
| Flutter is found at the setting, then on `PATH`, then in the homes — a setting that names nothing is no answer, a missing home is skipped; the system places come last and are the machine's only when asked for; the Android SDK is the setting, the named variables or Android Studio's folder | `resolve.rs::the_setting_wins_over_path_and_the_homes`, `a_missing_home_is_skipped_and_the_first_present_one_answers`, `the_system_places_come_last_and_are_the_machines_only_when_asked_for`, `the_android_sdk_is_the_setting_the_named_variables_or_android_studios_folder` |
| The fake records every call and moves a device it boots; a device it does not hold is `NoSuchDevice`, and a scripted failure is answered as scripted | `fake.rs::the_fake_records_every_call_and_moves_a_device_it_boots` |
| The crate depends on nothing of ours, and only the engine and the CLI depend on it | `crates/bisa-core/tests/it/layering.rs::allowed_edges` |

---

## Tests

Unit tests beside each module, on fixtures — `flutter --version --machine` output, doctor lines,
`simctl` and `adb` listings, a PNG header built in the test. `resolve.rs`'s tests read a `tempfile`
home through `Places::under`, so nothing of the machine is read; `exec.rs`'s ask for a program at a
path that does not exist, so nothing is spawned. What the engine composes over this crate is tested
in `crates/bisa-engine/tests/it/mobile_development.rs`, over `FakeMobileDevelopment`. No test of any
crate spawns a mobile program.
