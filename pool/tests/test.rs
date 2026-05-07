//! Smoke tests for the OotleSwap pool template.

use tari_template_test_tooling::TemplateTest;

#[test]
fn pool_template_compiles_and_loads() {
    // Compiles both this crate (the Pool template) and the sibling soon_token
    // crate. With no workspace, each package builds into its own target dir,
    // which is where test_tooling expects the .wasm artifact.
    let test = TemplateTest::new(".", [".", "../soon_token"]);
    let pool_addr = test.get_template_address("Pool");
    let soon_addr = test.get_template_address("SoonToken");
    assert_ne!(format!("{:?}", pool_addr), format!("{:?}", soon_addr));
}
