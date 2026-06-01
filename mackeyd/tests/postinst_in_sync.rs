//! The RPM scriptlets are inlined in Cargo.toml because cargo-generate-rpm
//! (0.21) can't include them from a file, while the .deb reads them from
//! packaging/maintainer/. This guards against the two copies drifting.

use std::fs;

fn rpm_scriptlet(field: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let cargo_toml = fs::read_to_string(format!("{manifest}/Cargo.toml")).expect("read Cargo.toml");
    let parsed: toml::Value = toml::from_str(&cargo_toml).expect("parse Cargo.toml");
    parsed["package"]["metadata"]["generate-rpm"][field]
        .as_str()
        .unwrap_or_else(|| panic!("{field} is an inline string"))
        .trim()
        .to_string()
}

fn deb_scriptlet(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    fs::read_to_string(format!("{manifest}/../packaging/maintainer/{name}"))
        .unwrap_or_else(|_| panic!("read packaging/maintainer/{name}"))
        .trim()
        .to_string()
}

#[test]
fn rpm_post_install_matches_deb_postinst() {
    assert_eq!(
        rpm_scriptlet("post_install_script"),
        deb_scriptlet("postinst"),
        "RPM post_install_script (Cargo.toml) and .deb postinst have drifted; keep them identical"
    );
}

#[test]
fn rpm_pre_uninstall_matches_deb_prerm() {
    assert_eq!(
        rpm_scriptlet("pre_uninstall_script"),
        deb_scriptlet("prerm"),
        "RPM pre_uninstall_script (Cargo.toml) and .deb prerm have drifted; keep them identical"
    );
}
