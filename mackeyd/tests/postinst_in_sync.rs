//! The RPM `%post` scriptlet is inlined in Cargo.toml because cargo-generate-rpm
//! (0.21) can't include it from a file, while the .deb reads it from
//! packaging/maintainer/postinst. This guards against the two copies drifting.

use std::fs;

#[test]
fn rpm_post_install_matches_deb_postinst() {
    let manifest = env!("CARGO_MANIFEST_DIR");

    let deb_postinst = fs::read_to_string(format!("{manifest}/../packaging/maintainer/postinst"))
        .expect("read packaging/maintainer/postinst");

    let cargo_toml = fs::read_to_string(format!("{manifest}/Cargo.toml")).expect("read Cargo.toml");
    let parsed: toml::Value = toml::from_str(&cargo_toml).expect("parse Cargo.toml");
    let rpm_script = parsed["package"]["metadata"]["generate-rpm"]["post_install_script"]
        .as_str()
        .expect("post_install_script is an inline string");

    assert_eq!(
        rpm_script.trim(),
        deb_postinst.trim(),
        "RPM post_install_script (Cargo.toml) and .deb postinst have drifted; keep them identical"
    );
}
