const with_astral = "a\u{1f600}b";
console.log(String(with_astral.length));
const astral_only = "\u{1f600}\u{1f600}";
console.log(String(astral_only.length));
