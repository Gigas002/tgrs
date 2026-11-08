use super::*;

#[test]
fn stderr_hides_info_but_honors_verbose() {
    assert_eq!(stderr_filter(LogLevel::Info), LevelFilter::WARN);
    assert_eq!(stderr_filter(LogLevel::Debug), LevelFilter::DEBUG);
    assert_eq!(stderr_filter(LogLevel::Error), LevelFilter::ERROR);
}

#[test]
fn level_filter_covers_off() {
    assert_eq!(level_filter(LogLevel::Off), LevelFilter::OFF);
    assert_eq!(level_filter(LogLevel::Trace), LevelFilter::TRACE);
}

#[test]
fn rotation_maps_every_variant() {
    assert_eq!(rotation(Rotation::Never), AppenderRotation::NEVER);
    assert_eq!(rotation(Rotation::Daily), AppenderRotation::DAILY);
}
