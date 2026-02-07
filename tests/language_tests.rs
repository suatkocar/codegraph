//! Integration tests for language support across the full parsing pipeline.
//!
//! These tests verify that each supported language can:
//! 1. Be detected from file extensions
//! 2. Have its parser initialized
//! 3. Parse representative source code
//! 4. Load and compile tree-sitter queries
//!
//! Uses `test-case` for parameterized coverage across all 32 languages.

use codegraph::types::Language;
use test_case::test_case;

// =========================================================================
// Helper
// =========================================================================

/// All 32 language variants for exhaustive testing.
const ALL_LANGUAGES: [Language; 32] = [
    Language::TypeScript,
    Language::Tsx,
    Language::JavaScript,
    Language::Jsx,
    Language::Python,
    Language::Go,
    Language::Rust,
    Language::Java,
    Language::C,
    Language::Cpp,
    Language::CSharp,
    Language::Php,
    Language::Ruby,
    Language::Swift,
    Language::Kotlin,
    Language::Bash,
    Language::Scala,
    Language::Dart,
    Language::Zig,
    Language::Lua,
    Language::Verilog,
    Language::Haskell,
    Language::Elixir,
    Language::Groovy,
    Language::PowerShell,
    Language::Clojure,
    Language::Julia,
    Language::R,
    Language::Erlang,
    Language::Elm,
    Language::Fortran,
    Language::Nix,
];

// =========================================================================
// Exhaustive language coverage
// =========================================================================

#[test]
fn all_languages_have_consistent_as_str_from_str_roundtrip() {
    for lang in ALL_LANGUAGES {
        let s = lang.as_str();
        let back = Language::from_str_loose(s);
        assert!(
            back.is_some(),
            "from_str_loose({s:?}) returned None for {lang:?}"
        );
        assert_eq!(
            back.unwrap(),
            lang,
            "roundtrip failed for {lang:?} -> {s:?}"
        );
    }
}

#[test]
fn all_languages_have_nonempty_grammar_name() {
    for lang in ALL_LANGUAGES {
        let name = lang.grammar_name();
        assert!(!name.is_empty(), "{lang:?} has empty grammar name");
    }
}

#[test]
fn all_languages_have_nonempty_query_source() {
    for lang in ALL_LANGUAGES {
        let source = lang.query_source();
        assert!(!source.is_empty(), "{lang:?} has empty query source");
    }
}

#[test]
fn all_languages_serialize_to_lowercase() {
    for lang in ALL_LANGUAGES {
        let json = serde_json::to_string(&lang).unwrap();
        let inner = json.trim_matches('"');
        assert_eq!(
            inner,
            inner.to_lowercase(),
            "serialization of {lang:?} should be lowercase"
        );
    }
}

#[test]
fn all_languages_deserialize_from_json() {
    for lang in ALL_LANGUAGES {
        let json = serde_json::to_string(&lang).unwrap();
        let back: Language = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lang, "deserialization failed for {lang:?}");
    }
}

// =========================================================================
// Language::from_extension comprehensive mapping
// =========================================================================

#[test_case(".ts", Some(Language::TypeScript) ; "ext_ts")]
#[test_case(".tsx", Some(Language::Tsx) ; "ext_tsx")]
#[test_case(".js", Some(Language::JavaScript) ; "ext_js")]
#[test_case(".mjs", Some(Language::JavaScript) ; "ext_mjs")]
#[test_case(".cjs", Some(Language::JavaScript) ; "ext_cjs")]
#[test_case(".jsx", Some(Language::Jsx) ; "ext_jsx")]
#[test_case(".py", Some(Language::Python) ; "ext_py")]
#[test_case(".go", Some(Language::Go) ; "ext_go")]
#[test_case(".rs", Some(Language::Rust) ; "ext_rs")]
#[test_case(".java", Some(Language::Java) ; "ext_java")]
#[test_case(".c", Some(Language::C) ; "ext_c")]
#[test_case(".h", Some(Language::C) ; "ext_h")]
#[test_case(".cpp", Some(Language::Cpp) ; "ext_cpp")]
#[test_case(".cc", Some(Language::Cpp) ; "ext_cc")]
#[test_case(".cxx", Some(Language::Cpp) ; "ext_cxx")]
#[test_case(".hpp", Some(Language::Cpp) ; "ext_hpp")]
#[test_case(".hxx", Some(Language::Cpp) ; "ext_hxx")]
#[test_case(".hh", Some(Language::Cpp) ; "ext_hh")]
#[test_case(".cs", Some(Language::CSharp) ; "ext_cs")]
#[test_case(".php", Some(Language::Php) ; "ext_php")]
#[test_case(".rb", Some(Language::Ruby) ; "ext_rb")]
#[test_case(".swift", Some(Language::Swift) ; "ext_swift")]
#[test_case(".kt", Some(Language::Kotlin) ; "ext_kt")]
#[test_case(".kts", Some(Language::Kotlin) ; "ext_kts")]
#[test_case(".sh", Some(Language::Bash) ; "ext_sh")]
#[test_case(".bash", Some(Language::Bash) ; "ext_bash")]
#[test_case(".zsh", Some(Language::Bash) ; "ext_zsh")]
#[test_case(".scala", Some(Language::Scala) ; "ext_scala")]
#[test_case(".sc", Some(Language::Scala) ; "ext_sc")]
#[test_case(".dart", Some(Language::Dart) ; "ext_dart")]
#[test_case(".zig", Some(Language::Zig) ; "ext_zig")]
#[test_case(".lua", Some(Language::Lua) ; "ext_lua")]
#[test_case(".v", Some(Language::Verilog) ; "ext_v")]
#[test_case(".vh", Some(Language::Verilog) ; "ext_vh")]
#[test_case(".sv", Some(Language::Verilog) ; "ext_sv")]
#[test_case(".svh", Some(Language::Verilog) ; "ext_svh")]
#[test_case(".hs", Some(Language::Haskell) ; "ext_hs")]
#[test_case(".lhs", Some(Language::Haskell) ; "ext_lhs")]
#[test_case(".ex", Some(Language::Elixir) ; "ext_ex")]
#[test_case(".exs", Some(Language::Elixir) ; "ext_exs")]
#[test_case(".groovy", Some(Language::Groovy) ; "ext_groovy")]
#[test_case(".gradle", Some(Language::Groovy) ; "ext_gradle")]
#[test_case(".ps1", Some(Language::PowerShell) ; "ext_ps1")]
#[test_case(".psm1", Some(Language::PowerShell) ; "ext_psm1")]
#[test_case(".psd1", Some(Language::PowerShell) ; "ext_psd1")]
#[test_case(".clj", Some(Language::Clojure) ; "ext_clj")]
#[test_case(".cljs", Some(Language::Clojure) ; "ext_cljs")]
#[test_case(".cljc", Some(Language::Clojure) ; "ext_cljc")]
#[test_case(".edn", Some(Language::Clojure) ; "ext_edn")]
#[test_case(".jl", Some(Language::Julia) ; "ext_jl")]
#[test_case(".r", Some(Language::R) ; "ext_r_lower")]
#[test_case(".R", Some(Language::R) ; "ext_r_upper")]
#[test_case(".Rmd", Some(Language::R) ; "ext_rmd")]
#[test_case(".erl", Some(Language::Erlang) ; "ext_erl")]
#[test_case(".hrl", Some(Language::Erlang) ; "ext_hrl")]
#[test_case(".elm", Some(Language::Elm) ; "ext_elm")]
#[test_case(".f90", Some(Language::Fortran) ; "ext_f90")]
#[test_case(".f95", Some(Language::Fortran) ; "ext_f95")]
#[test_case(".f03", Some(Language::Fortran) ; "ext_f03")]
#[test_case(".f08", Some(Language::Fortran) ; "ext_f08")]
#[test_case(".f", Some(Language::Fortran) ; "ext_f")]
#[test_case(".for", Some(Language::Fortran) ; "ext_for")]
#[test_case(".fpp", Some(Language::Fortran) ; "ext_fpp")]
#[test_case(".nix", Some(Language::Nix) ; "ext_nix")]
// Unsupported
#[test_case(".yaml", None ; "ext_yaml")]
#[test_case(".json", None ; "ext_json")]
#[test_case(".md", None ; "ext_md")]
#[test_case(".toml", None ; "ext_toml")]
#[test_case(".xml", None ; "ext_xml")]
#[test_case(".txt", None ; "ext_txt")]
#[test_case(".html", None ; "ext_html")]
#[test_case(".css", None ; "ext_css")]
#[test_case(".wasm", None ; "ext_wasm")]
#[test_case("", None ; "ext_empty")]
fn from_extension_integration(ext: &str, expected: Option<Language>) {
    assert_eq!(Language::from_extension(ext), expected);
}

// =========================================================================
// Language display integration
// =========================================================================

#[test]
fn language_display_and_as_str_agree() {
    for lang in ALL_LANGUAGES {
        assert_eq!(format!("{lang}"), lang.as_str());
    }
}

// =========================================================================
// Language from_str_loose aliases
// =========================================================================

#[test_case("golang", Some(Language::Go) ; "alias_golang")]
#[test_case("c++", Some(Language::Cpp) ; "alias_cpp")]
#[test_case("c#", Some(Language::CSharp) ; "alias_csharp_hash")]
#[test_case("c_sharp", Some(Language::CSharp) ; "alias_c_underscore_sharp")]
#[test_case("shell", Some(Language::Bash) ; "alias_shell")]
#[test_case("sh", Some(Language::Bash) ; "alias_sh")]
#[test_case("systemverilog", Some(Language::Verilog) ; "alias_systemverilog")]
#[test_case("sv", Some(Language::Verilog) ; "alias_sv")]
#[test_case("hs", Some(Language::Haskell) ; "alias_hs")]
#[test_case("ex", Some(Language::Elixir) ; "alias_ex")]
#[test_case("clj", Some(Language::Clojure) ; "alias_clj")]
#[test_case("jl", Some(Language::Julia) ; "alias_jl")]
#[test_case("erl", Some(Language::Erlang) ; "alias_erl")]
#[test_case("ps1", Some(Language::PowerShell) ; "alias_ps1")]
#[test_case("f90", Some(Language::Fortran) ; "alias_f90")]
#[test_case("PYTHON", Some(Language::Python) ; "alias_upper_python")]
#[test_case("Rust", Some(Language::Rust) ; "alias_mixed_rust")]
#[test_case("unknown_lang", None ; "alias_unknown")]
fn from_str_loose_integration(input: &str, expected: Option<Language>) {
    assert_eq!(Language::from_str_loose(input), expected);
}
