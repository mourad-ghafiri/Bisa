# bisa-mobile-development

The mobile toolchain and the devices on this machine, as typed, time-boxed, argv-only subprocess
calls — `flutter`, `xcrun simctl`, `xcodebuild`, `adb`, `emulator`, `pod`, `java` — a leaf crate
like `bisa-ssh`, with no dependency on anything of ours
([ide/19](../ide/19-mobile-development.md)). It says what is installed for Flutter development on
iOS and Android and what Flutter's own doctor says, lists the simulators, emulators and phones this
machine can reach, boots and shuts a simulator or an emulator down, brings the Simulator window
forward, makes a simulator, and captures a device's screen as PNG or JPEG bytes. It composes the
one `flutter run` line a terminal runs and never runs it: the app runs where a person watches it.
The tools are a port with a fake every caller's test hands in; the real ones are held by this
crate's own tests against scripted stand-ins of each program. No test of any crate spawns a
mobile program of this machine.

---

## Where things live

| Module | Owns |
|---|---|
| `lib.rs` | the types — `Platform { Ios, Android }`, `DeviceKind { Simulator, Emulator, Physical }`, `DeviceState { Booted, Shutdown, Running, Offline, Booting }` (`is_up` is booted or running), `ImageFormat { Png, Jpeg }`, `Device { id, name, platform, kind, state, os?, image? }` (`words`) — `image` is the emulator image's name: a shut-down emulator's id is that name, a running one's id is its `adb` serial, and the image says the two are one device — `Toolchain { flutter: FlutterInfo, xcode: XcodeInfo, ios_runtimes, ios_devicetypes, cocoapods: ToolInfo, android: AndroidSdk, java: ToolInfo, doctor: Vec<DoctorLine>, unusable_hints: Vec<Hint> }` (a setting that names nothing is said, never passed over), `Hints { flutter_path?, android_sdk? }` (the two settings, passed per call), `Listing { devices, problems: Vec<ListingProblem { platform, error }> }`, `MobileDevelopmentError { NotInstalled, Timeout, Failed, NoSuchDevice, Unsupported }` — and the port `MobileDevelopmentTools: Send + Sync + Debug` (`toolchain`, `flutter_path`, `devices`, `listing(hints, only)` with a default over `devices`, `boot` and `shutdown` answering the `Device` as it then stands, `show`, `create_simulator`, `screenshot`), synchronous, driven from `spawn_blocking`; `run_command(flutter, device)` (the line, `shlex`-quoted — an id or a path no quoting carries is refused, never written bare), `sort_for_a_picker` (up first, then platform, then name), `png_size(bytes)` (the IHDR) |
| `exec.rs` | the one process launcher: `Exec { timeouts: Timeouts { local 15 s, doctor 90 s, boot 120 s, shutdown 15 s, poll 2 s, settle 2 s }, limits: Limits { stdout 48 MiB, stderr 64 KiB } }`, `run(program: &Path, args, env, timeout) -> Output { code, stdout: Vec<u8>, stderr, clipped }` — by absolute path, argv only, stdin null, a process group of its own, `HARDENED_ENV` (`NO_COLOR`, `TERM=dumb`, `LC_ALL=C`, `PAGER=cat`, `FLUTTER_SUPPRESS_ANALYTICS`, `CI`) over the process's own; on expiry the child **and its whole group** are terminated; what it writes is read up to the caps on a thread per pipe, the rest read and dropped (`clipped`); a finished child's pipes are read for `settle` and no longer, so something it started that still holds them never holds the call; `NotFound` is `NotInstalled`, a file that cannot be run is `Failed`; `spawn_detached(program, args, env) -> Detached` for a program that keeps a window — every stream null, `exited()` while the handle is held, and a thread that waits for its end once the handle goes, so it is never a zombie |
| `resolve.rs` | where the programs are, pure over injected probes: `flutter_path(hint, places, on_path)` — the setting (a file, or a folder taken as the Flutter home; `~` is the home), then `PATH`, then the `Places`: `FLUTTER_HOMES` under its home, then its system folders; `android_sdk(hint, env, home)` — the setting, `ANDROID_HOME` then `ANDROID_SDK_ROOT` read by name, then Android Studio's folder (`Library/Android/sdk`, then Linux's `Android/Sdk`); `adb_of`, `emulator_of` under the SDK; `Places { home, system, path }` — `machine(home)` is the home, Homebrew's folders and the process's `PATH`; `under(home)` has no system folder; `sealed(home, folders)` looks for every program in the named folders alone, reads no variable and no fixed system program — what the tests of `Real` use; `program(name)`, `fixed(path)`, `expand`, `said` |
| `parse.rs` | each program's output read once into a typed value, on fixtures: `flutter_version` (`--version --machine`, after any notice), `doctor_lines` (`[✓]`/`[✗]`/`[!]` and the Windows marks, the name before the dash, the detail in the parentheses; a one-character mark nobody knows is kept as a warning), `simctl_devices` (iOS runtimes alone, the OS from the runtime's id, unavailable ones dropped, `Booting` read as such, an entry that does not parse left out alone), `simctl_runtimes`, `simctl_devicetypes` (iPhone and iPad), `adb_devices` (`-l`: `emulator-NNNN` an emulator, else a phone; `model:` the name; `offline` and `unauthorized` both offline), `avd_list`, `avd_name`, `xcodebuild_version`, `xcode_select`, `pod_version`, `java_version` (stderr) |
| `real.rs` | `Real { exec, places }` (`Default` is this machine's) — the port over the real programs. `toolchain` probes every part on its own and never errors; Flutter's own probes are told the SDK (`ANDROID_HOME`) the examination found. Nothing goes through `xcrun` or `xcodebuild` unless `xcode-select -p` names a developer directory. `listing(hints, only)` is the available simulators, `adb`'s list with each emulator named after the image its console says it runs (`image`), and every image nobody started as a shut-down emulator — a tool that fails is a `ListingProblem` beside the other platform's devices, and a platform not asked for is not asked. `boot`: a simulator that is up is answered as it stands; else `simctl boot` (exit 149 is *already booted*; skipped for one already booting) and `bootstatus -b`, both read; an emulator's image is started detached and looked for every `poll` until `adb` lists a running emulator of that image — answered **as that device**, its serial for an id — while an emulator that ends with a failure first is said at once, and one that is there and not answering is refused. `shutdown` is `simctl shutdown`, or `adb emu kill` with its exit read and the serial waited out of `adb`'s list, the emulator answered as its image again. `show` runs `open -a Simulator` and reads its exit. `create_simulator` is `simctl create`, the UDID answered. `screenshot` refuses a device that is not up before any tool is asked, then `simctl io <udid> screenshot --type=<fmt>` into a `NamedTempFile` read and dropped, or `adb exec-out screencap -p` (PNG whatever was asked; a capture past the cap is refused). A phone is not booted, shut down or shown from here (`Unsupported`) |
| `fake.rs` | `FakeMobileDevelopment` — the scripted port every caller's test hands in: `with_toolchain`, `with_devices`, `with_screenshot`, `failing(op, error)`, `calls()`. It behaves as `Real` does where a caller could tell: a listing in a picker's order; an emulator that boots answered under an `emulator-NNNN` serial and back under its image when shut down; no capture from a device that is not up; a phone neither booted nor shut down; a window shown for iOS alone. `png_fixture(width, height)`, `simulator(...)`, `emulator(...)`, `phone(...)` for a test's rows |

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
| A program that is not there is *not installed*, for a run and for a detached spawn alike, and a file that cannot be run is a failure; an `Output` prints its size and never its bytes | `exec.rs::a_program_that_is_not_there_is_said_to_be_not_installed`, `a_file_that_cannot_be_run_is_a_failure_in_the_systems_words`, `an_output_says_its_size_and_not_its_bytes_and_complains_in_the_programs_words` |
| A budget that runs out ends what the child started, not the child alone; a finished child's pipes are read for a moment, not for a helper's life; what is written past the cap is dropped and said; a detached program is watched while held and left running after | `exec.rs::a_budget_that_runs_out_ends_what_the_child_started_too`, `a_finished_childs_pipes_are_read_for_a_moment_not_for_a_helpers_life`, `what_is_written_past_the_cap_is_dropped_and_said`, `a_detached_program_is_watched_while_held_and_left_running_after` |
| A shut-down simulator is booted and waited for, one that is up is left; `simctl`'s refusal is a failure in its words | `real.rs::a_shut_down_simulator_is_booted_and_waited_for`, `a_boot_simctl_refuses_is_a_failure_in_its_words` |
| An emulator's image boots into the device `adb` lists and keeps its image; one that ends first is a failure at once, one that never appears a timeout; what cannot be booted is refused before anything is started | `real.rs::an_emulator_boots_into_the_device_adb_lists_and_keeps_its_image`, `an_emulator_that_ends_before_its_device_is_up_is_a_failure_at_once`, `an_emulator_whose_device_never_appears_times_out`, `what_cannot_be_booted_is_refused_before_anything_is_started` |
| A shutdown reads the tool's answer; a listing says what it could not ask and asks only what it was asked; a screen is captured from a device that is up and refused from one that is not, and a capture past the cap is refused; `open`'s refusal is read | `real.rs::a_shutdown_reads_the_tools_answer`, `a_listing_says_what_it_could_not_ask_and_asks_only_what_it_was_asked`, `what_adb_lists_is_named_after_its_image_when_the_console_answers`, `a_screen_is_captured_from_a_device_that_is_up_and_refused_from_one_that_is_not`, `a_capture_past_the_cap_is_refused`, `the_simulators_window_is_shown_and_opens_refusal_is_read` |
| The setup is examined part by part; a setting that names nothing is said, and what is absent is absent alone | `real.rs::the_setup_is_examined_part_by_part`, `a_setting_that_names_nothing_is_said_and_what_is_absent_is_absent_alone` |
| The run line quotes a path with spaces and the device's id, and refuses what no quoting carries; a PNG's size is read off its IHDR and anything else is refused; a device says itself in one line; a picker reads what is up first | `lib.rs::run_command_quotes_a_path_with_spaces_and_the_device_id`, `an_id_or_a_path_no_quoting_carries_is_refused`, `png_size_reads_the_ihdr_and_refuses_anything_else`, `a_device_says_itself_in_one_line`, `a_picker_reads_what_is_up_first_then_platform_then_name` |
| Each program's output is read once into a typed value: Flutter's version after any notice, the doctor's category lines, the iOS simulators with their OS, the iOS runtimes and device types, `adb`'s emulators, phones and offline ones, the small programs' answers | `parse.rs::flutters_version_is_read_off_the_machine_object_even_after_a_notice`, `the_doctors_category_lines_are_read_and_its_detail_lines_left`, `simulators_are_read_with_their_os_and_the_other_platforms_left`, `runtimes_and_device_types_are_read_for_ios_alone`, `adb_lists_emulators_phones_and_the_offline_ones`, `the_small_programs_answers_are_read` |
| Flutter is found at the setting, then on `PATH`, then in the homes — a setting that names nothing is no answer, a missing home is skipped; the system places come last and are the machine's only when asked for; the Android SDK is the setting, the named variables or Android Studio's folder | `resolve.rs::the_setting_wins_over_path_and_the_homes`, `a_missing_home_is_skipped_and_the_first_present_one_answers`, `the_system_places_come_last_and_are_the_machines_only_when_asked_for`, `the_android_sdk_is_the_setting_the_named_variables_or_android_studios_folder` |
| The fake records every call and moves a device it boots; a device it does not hold is `NoSuchDevice`, and a scripted failure is answered as scripted; an emulator boots to a serial and back to its image, a phone is not booted from here | `fake.rs::the_fake_records_every_call_and_moves_a_device_it_boots`, `an_emulator_boots_to_running_a_phone_is_not_booted_from_here_and_a_shutdown_lands` |
| The crate depends on nothing of ours, and only the engine and the CLI depend on it | `crates/bisa-core/tests/it/layering.rs::allowed_edges` |

---

## Tests

Unit tests beside each module, on fixtures and, for the runner, against the machine's own `/bin/sh` by absolute path (what it wrote read whole, its exit status an answer, a child past its budget ended with what it started, one started detached watched and left running) — `flutter --version --machine` output, doctor lines,
`simctl` and `adb` listings, a PNG header built in the test. `resolve.rs`'s tests read a `tempfile`
home, so nothing of the machine is read. `real.rs`'s tests drive `Real` end to end in sealed places
(`Places::sealed`): a stage of shell scripts standing in for `xcode-select`, `xcrun`, `xcodebuild`,
`pod`, `java`, `open`, `flutter`, `adb` and `emulator`, each answering what the vendor's tool
answers and recording what it was asked, so a test asserts the exact arguments — and a program of
this machine is never found. What the engine composes over this crate is tested
in `crates/bisa-engine/tests/it/mobile_development.rs`, over `FakeMobileDevelopment`. No test of any
crate spawns a mobile program of this machine.
