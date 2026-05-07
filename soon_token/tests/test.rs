//! Smoke tests for the $SOON token template.

use tari_template_test_tooling::TemplateTest;

#[test]
fn soon_template_compiles_and_loads() {
    let test = TemplateTest::new(".", ["."]);
    let _addr = test.get_template_address("SoonToken");
}
