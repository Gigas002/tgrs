use std::{collections::HashMap, ffi::OsString, path::Path};

use super::*;

fn lookup(vars: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
    let vars: HashMap<&str, OsString> = vars.iter().map(|(k, v)| (*k, (*v).into())).collect();
    move |key| vars.get(key).cloned()
}

#[test]
fn fallbacks_without_xdg_vars() {
    let xdg = XdgDirs::from_lookup(&lookup(&[("HOME", "/home/u")])).unwrap();

    assert_eq!(xdg.config_home, Path::new("/home/u/.config"));
    assert_eq!(xdg.data_home, Path::new("/home/u/.local/share"));
    assert_eq!(xdg.state_home, Path::new("/home/u/.local/state"));
}

#[test]
fn xdg_vars_override_fallbacks() {
    let xdg = XdgDirs::from_lookup(&lookup(&[
        ("HOME", "/home/u"),
        ("XDG_CONFIG_HOME", "/cfg"),
        ("XDG_DATA_HOME", "/data"),
        ("XDG_STATE_HOME", "/state"),
    ]))
    .unwrap();
    let dirs = AppDirs::new(&xdg);

    assert_eq!(dirs.config, Path::new("/cfg/tgt"));
    assert_eq!(dirs.data, Path::new("/data/tgt"));
    assert_eq!(dirs.state, Path::new("/state/tgt"));
}

#[test]
fn relative_xdg_vars_are_ignored() {
    let xdg = XdgDirs::from_lookup(&lookup(&[
        ("HOME", "/home/u"),
        ("XDG_CONFIG_HOME", "relative"),
    ]))
    .unwrap();

    assert_eq!(xdg.config_home, Path::new("/home/u/.config"));
}

#[test]
fn missing_home_is_an_error() {
    assert_eq!(
        XdgDirs::from_lookup(&lookup(&[])).unwrap_err(),
        PathsError::NoHome
    );
}

#[test]
fn expand_tilde_only_expands_leading_tilde() {
    let home = Path::new("/home/u");

    assert_eq!(expand_tilde(Path::new("~"), home), home);
    assert_eq!(expand_tilde(Path::new("~/tg"), home), home.join("tg"));
    assert_eq!(
        expand_tilde(Path::new("~other/tg"), home),
        Path::new("~other/tg")
    );
    assert_eq!(expand_tilde(Path::new("/abs"), home), Path::new("/abs"));
}
