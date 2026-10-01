//! The journeys: the platform from end to end, through the binary a person
//! runs — the real `bisa`, the real node over its socket, the real MCP server
//! — with no window and no browser. Each starts a daemon over a sealed
//! workspace ([`sealed`]), does what a person does with the command line,
//! lets scripted agents do what agents do, and reads the result back from
//! the verbs and from the files.
//!
//! A journey asserts the promise a feature makes to a person, never how the
//! code keeps it: what the inbox asks, what a run came to, what is on disk
//! after the daemon is gone. One journey a file, named after what it proves.

mod sealed;

mod a_branch_goes_out;
mod a_channel_and_what_is_said_in_it;
mod a_conversation_about_a_checkout;
mod a_folder_served_and_a_browser_asked;
mod a_harness_in_a_terminal;
mod a_platform_reached_through_a_connector;
mod addons_drawings_and_notes;
mod copilot_and_grok;
mod crash_in_a_step;
mod events_and_gateways;
mod every_step_kind;
mod files_and_search;
mod from_capture_to_done;
mod git_in_a_checkout;
mod goals;
mod models_and_effort;
mod projects_and_workstreams;
mod pulse_and_inbox;
mod runs_in_the_workspace;
mod settings_security_and_the_log;
mod templates_installed_and_run;
mod the_decision_making_agent;
mod the_nodes_wire;
mod the_platforms_tools;
mod the_rest_of_the_command_line;
mod the_roster_and_its_library;
mod the_setup_gate;
mod two_nodes_and_a_relay;
mod two_verbs_against_one_daemon;
mod words_from_the_command_line;
