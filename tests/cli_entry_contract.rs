#[test]
fn no_arguments_remain_desktop_mode() {
    assert!(!tokensaver::should_run_cli(&[]));
}

#[test]
fn normal_cli_command_selects_cli_mode() {
    assert!(tokensaver::should_run_cli(&["status".to_owned()]));
}

#[test]
fn macos_process_serial_number_argument_does_not_select_cli_mode() {
    assert!(!tokensaver::should_run_cli(&["-psn_0_12345".to_owned()]));
}
