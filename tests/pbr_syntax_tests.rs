use symbios::parser::{parse_module, parse_rule};

#[test]
fn test_parse_pbr_symbols() {
    // Color: '
    let (_, m) = parse_module("'(1, 0, 0)").expect("Should parse color");
    assert_eq!(m.symbol, "'");
    assert_eq!(m.params.len(), 3);

    // Material: ,
    let (_, m) = parse_module(",(2)").expect("Should parse material");
    assert_eq!(m.symbol, ",");

    // Metallic: @
    let (_, m) = parse_module("@(0.8)").expect("Should parse metallic");
    assert_eq!(m.symbol, "@");

    // Roughness: #
    let (_, m) = parse_module("#(0.1)").expect("Should parse roughness");
    assert_eq!(m.symbol, "#");
}

#[test]
fn test_pbr_rule_syntax() {
    let input = "A(x) -> '(1,0,0) F(x) ,(1) F(x)";
    let (_, rule) = parse_rule(input).expect("Should parse PBR rule");

    assert_eq!(rule.successors[0].symbol, "'");
    assert_eq!(rule.successors[2].symbol, ",");
}
