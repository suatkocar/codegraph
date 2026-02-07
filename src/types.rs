//! Core domain types for CodeGraph.
//!
//! Faithfully mirrors the TypeScript `types.ts` to ensure database and
//! API compatibility between the TS and Rust versions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// Supported source languages (32 languages, 35 variants counting JSX/TSX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Python,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    CSharp,
    Php,
    Ruby,
    Swift,
    Kotlin,
    // Phase 11: 17 new languages
    Bash,
    Scala,
    Dart,
    Zig,
    Lua,
    Verilog,
    Haskell,
    Elixir,
    Groovy,
    PowerShell,
    Clojure,
    Julia,
    R,
    Erlang,
    Elm,
    Fortran,
    Nix,
}

impl Language {
    /// Map a file extension (including the dot) to a language.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            ".ts" => Some(Self::TypeScript),
            ".tsx" => Some(Self::Tsx),
            ".js" | ".mjs" | ".cjs" => Some(Self::JavaScript),
            ".jsx" => Some(Self::Jsx),
            ".py" => Some(Self::Python),
            ".go" => Some(Self::Go),
            ".rs" => Some(Self::Rust),
            ".java" => Some(Self::Java),
            ".c" | ".h" => Some(Self::C),
            ".cpp" | ".cc" | ".cxx" | ".hpp" | ".hxx" | ".hh" => Some(Self::Cpp),
            ".cs" => Some(Self::CSharp),
            ".php" => Some(Self::Php),
            ".rb" => Some(Self::Ruby),
            ".swift" => Some(Self::Swift),
            ".kt" | ".kts" => Some(Self::Kotlin),
            // Phase 11
            ".sh" | ".bash" | ".zsh" => Some(Self::Bash),
            ".scala" | ".sc" => Some(Self::Scala),
            ".dart" => Some(Self::Dart),
            ".zig" => Some(Self::Zig),
            ".lua" => Some(Self::Lua),
            ".v" | ".vh" | ".sv" | ".svh" => Some(Self::Verilog),
            ".hs" | ".lhs" => Some(Self::Haskell),
            ".ex" | ".exs" => Some(Self::Elixir),
            ".groovy" | ".gradle" => Some(Self::Groovy),
            ".ps1" | ".psm1" | ".psd1" => Some(Self::PowerShell),
            ".clj" | ".cljs" | ".cljc" | ".edn" => Some(Self::Clojure),
            ".jl" => Some(Self::Julia),
            ".r" | ".R" | ".Rmd" => Some(Self::R),
            ".erl" | ".hrl" => Some(Self::Erlang),
            ".elm" => Some(Self::Elm),
            ".f90" | ".f95" | ".f03" | ".f08" | ".f" | ".for" | ".fpp" => Some(Self::Fortran),
            ".nix" => Some(Self::Nix),
            _ => None,
        }
    }

    /// The tree-sitter grammar name for loading the correct language.
    pub fn grammar_name(&self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript | Self::Jsx => "javascript",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "c_sharp",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            // Phase 11
            Self::Bash => "bash",
            Self::Scala => "scala",
            Self::Dart => "dart",
            Self::Zig => "zig",
            Self::Lua => "lua",
            Self::Verilog => "verilog",
            Self::Haskell => "haskell",
            Self::Elixir => "elixir",
            Self::Groovy => "groovy",
            Self::PowerShell => "powershell",
            Self::Clojure => "clojure",
            Self::Julia => "julia",
            Self::R => "r",
            Self::Erlang => "erlang",
            Self::Elm => "elm",
            Self::Fortran => "fortran",
            Self::Nix => "nix",
        }
    }

    /// Embedded `.scm` query source for this language.
    pub fn query_source(&self) -> &'static str {
        match self {
            Self::TypeScript | Self::Tsx => include_str!("../queries/typescript.scm"),
            Self::JavaScript | Self::Jsx => include_str!("../queries/javascript.scm"),
            Self::Python => include_str!("../queries/python.scm"),
            Self::Go => include_str!("../queries/go.scm"),
            Self::Rust => include_str!("../queries/rust.scm"),
            Self::Java => include_str!("../queries/java.scm"),
            Self::C => include_str!("../queries/c.scm"),
            Self::Cpp => include_str!("../queries/cpp.scm"),
            Self::CSharp => include_str!("../queries/csharp.scm"),
            Self::Php => include_str!("../queries/php.scm"),
            Self::Ruby => include_str!("../queries/ruby.scm"),
            Self::Swift => include_str!("../queries/swift.scm"),
            Self::Kotlin => include_str!("../queries/kotlin.scm"),
            // Phase 11
            Self::Bash => include_str!("../queries/bash.scm"),
            Self::Scala => include_str!("../queries/scala.scm"),
            Self::Dart => include_str!("../queries/dart.scm"),
            Self::Zig => include_str!("../queries/zig.scm"),
            Self::Lua => include_str!("../queries/lua.scm"),
            Self::Verilog => include_str!("../queries/verilog.scm"),
            Self::Haskell => include_str!("../queries/haskell.scm"),
            Self::Elixir => include_str!("../queries/elixir.scm"),
            Self::Groovy => include_str!("../queries/groovy.scm"),
            Self::PowerShell => include_str!("../queries/powershell.scm"),
            Self::Clojure => include_str!("../queries/clojure.scm"),
            Self::Julia => include_str!("../queries/julia.scm"),
            Self::R => include_str!("../queries/r.scm"),
            Self::Erlang => include_str!("../queries/erlang.scm"),
            Self::Elm => include_str!("../queries/elm.scm"),
            Self::Fortran => include_str!("../queries/fortran.scm"),
            Self::Nix => include_str!("../queries/nix.scm"),
        }
    }

    /// String representation matching the TS version's serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::Python => "python",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            // Phase 11
            Self::Bash => "bash",
            Self::Scala => "scala",
            Self::Dart => "dart",
            Self::Zig => "zig",
            Self::Lua => "lua",
            Self::Verilog => "verilog",
            Self::Haskell => "haskell",
            Self::Elixir => "elixir",
            Self::Groovy => "groovy",
            Self::PowerShell => "powershell",
            Self::Clojure => "clojure",
            Self::Julia => "julia",
            Self::R => "r",
            Self::Erlang => "erlang",
            Self::Elm => "elm",
            Self::Fortran => "fortran",
            Self::Nix => "nix",
        }
    }

    /// Parse from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "typescript" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "javascript" => Some(Self::JavaScript),
            "jsx" => Some(Self::Jsx),
            "python" => Some(Self::Python),
            "go" | "golang" => Some(Self::Go),
            "rust" => Some(Self::Rust),
            "java" => Some(Self::Java),
            "c" => Some(Self::C),
            "cpp" | "c++" => Some(Self::Cpp),
            "csharp" | "c#" | "c_sharp" => Some(Self::CSharp),
            "php" => Some(Self::Php),
            "ruby" => Some(Self::Ruby),
            "swift" => Some(Self::Swift),
            "kotlin" => Some(Self::Kotlin),
            // Phase 11
            "bash" | "shell" | "sh" => Some(Self::Bash),
            "scala" => Some(Self::Scala),
            "dart" => Some(Self::Dart),
            "zig" => Some(Self::Zig),
            "lua" => Some(Self::Lua),
            "verilog" | "systemverilog" | "sv" => Some(Self::Verilog),
            "haskell" | "hs" => Some(Self::Haskell),
            "elixir" | "ex" => Some(Self::Elixir),
            "groovy" => Some(Self::Groovy),
            "powershell" | "ps1" => Some(Self::PowerShell),
            "clojure" | "clj" => Some(Self::Clojure),
            "julia" | "jl" => Some(Self::Julia),
            "r" => Some(Self::R),
            "erlang" | "erl" => Some(Self::Erlang),
            "elm" => Some(Self::Elm),
            "fortran" | "f90" => Some(Self::Fortran),
            "nix" => Some(Self::Nix),
            _ => None,
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// NodeKind
// ---------------------------------------------------------------------------

/// Kinds of symbol nodes in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Function,
    Class,
    Method,
    Interface,
    TypeAlias,
    Enum,
    Variable,
    Struct,
    Trait,
    Module,
    Property,
    Namespace,
    Constant,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Method => "method",
            Self::Interface => "interface",
            Self::TypeAlias => "type_alias",
            Self::Enum => "enum",
            Self::Variable => "variable",
            Self::Struct => "struct",
            Self::Trait => "trait",
            Self::Module => "module",
            Self::Property => "property",
            Self::Namespace => "namespace",
            Self::Constant => "constant",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "function" => Some(Self::Function),
            "class" => Some(Self::Class),
            "method" => Some(Self::Method),
            "interface" => Some(Self::Interface),
            "type_alias" => Some(Self::TypeAlias),
            "enum" => Some(Self::Enum),
            "variable" => Some(Self::Variable),
            "struct" => Some(Self::Struct),
            "trait" => Some(Self::Trait),
            "module" => Some(Self::Module),
            "property" | "field" => Some(Self::Property),
            "namespace" | "package" => Some(Self::Namespace),
            "constant" | "const" => Some(Self::Constant),
            _ => None,
        }
    }
}

impl std::fmt::Display for NodeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// EdgeKind
// ---------------------------------------------------------------------------

/// Kinds of edges (relationships) between nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Imports,
    Calls,
    Contains,
    Extends,
    Implements,
    References,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::Contains => "contains",
            Self::Extends => "extends",
            Self::Implements => "implements",
            Self::References => "references",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "imports" => Some(Self::Imports),
            "calls" => Some(Self::Calls),
            "contains" => Some(Self::Contains),
            "extends" => Some(Self::Extends),
            "implements" => Some(Self::Implements),
            "references" => Some(Self::References),
            _ => None,
        }
    }
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// CodeNode
// ---------------------------------------------------------------------------

/// A symbol node in the code graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeNode {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
    pub kind: NodeKind,
    pub file_path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub language: Language,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exported: Option<bool>,
}

// ---------------------------------------------------------------------------
// CodeEdge
// ---------------------------------------------------------------------------

/// A relationship edge between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEdge {
    pub source: String,
    pub target: String,
    pub kind: EdgeKind,
    pub file_path: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Build a deterministic node ID: `{kind}:{filePath}:{name}:{startLine}`
///
/// Matches the TS version's `makeNodeId()` exactly.
pub fn make_node_id(kind: NodeKind, file_path: &str, name: &str, start_line: u32) -> String {
    format!("{}:{}:{}:{}", kind.as_str(), file_path, name, start_line)
}

// ---------------------------------------------------------------------------
// ParseResult
// ---------------------------------------------------------------------------

/// Result of parsing and extracting symbols from a single file.
pub struct ParseResult {
    pub file_path: String,
    pub language: Language,
    pub nodes: Vec<CodeNode>,
    pub edges: Vec<CodeEdge>,
    pub content_hash: String,
}

// ---------------------------------------------------------------------------
// FileRecord
// ---------------------------------------------------------------------------

/// File indexing metadata stored in the file_hashes table.
pub struct FileRecord {
    pub file_path: String,
    pub language: Language,
    pub content_hash: String,
    pub indexed_at: i64,
    pub node_count: usize,
    pub edge_count: usize,
}

// ---------------------------------------------------------------------------
// UnresolvedRef
// ---------------------------------------------------------------------------

/// An import or reference that could not be resolved to a known symbol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedRef {
    pub id: i64,
    pub source_id: String,
    pub specifier: String,
    pub ref_type: String,
    pub file_path: String,
    pub line: u32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_node_id() {
        let id = make_node_id(NodeKind::Function, "src/main.ts", "hello", 10);
        assert_eq!(id, "function:src/main.ts:hello:10");
    }

    #[test]
    fn test_language_from_extension() {
        // Original 15 languages
        assert_eq!(Language::from_extension(".ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension(".tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_extension(".js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension(".jsx"), Some(Language::Jsx));
        assert_eq!(Language::from_extension(".py"), Some(Language::Python));
        assert_eq!(Language::from_extension(".go"), Some(Language::Go));
        assert_eq!(Language::from_extension(".rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension(".java"), Some(Language::Java));
        assert_eq!(Language::from_extension(".c"), Some(Language::C));
        assert_eq!(Language::from_extension(".h"), Some(Language::C));
        assert_eq!(Language::from_extension(".cpp"), Some(Language::Cpp));
        assert_eq!(Language::from_extension(".hpp"), Some(Language::Cpp));
        assert_eq!(Language::from_extension(".cs"), Some(Language::CSharp));
        assert_eq!(Language::from_extension(".php"), Some(Language::Php));
        assert_eq!(Language::from_extension(".rb"), Some(Language::Ruby));
        assert_eq!(Language::from_extension(".kt"), Some(Language::Kotlin));
        assert_eq!(Language::from_extension(".kts"), Some(Language::Kotlin));
        assert_eq!(Language::from_extension(".swift"), Some(Language::Swift));
        assert_eq!(Language::from_extension(".mjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension(".cjs"), Some(Language::JavaScript));
        // Phase 11: 17 new languages
        assert_eq!(Language::from_extension(".sh"), Some(Language::Bash));
        assert_eq!(Language::from_extension(".bash"), Some(Language::Bash));
        assert_eq!(Language::from_extension(".zsh"), Some(Language::Bash));
        assert_eq!(Language::from_extension(".scala"), Some(Language::Scala));
        assert_eq!(Language::from_extension(".sc"), Some(Language::Scala));
        assert_eq!(Language::from_extension(".dart"), Some(Language::Dart));
        assert_eq!(Language::from_extension(".zig"), Some(Language::Zig));
        assert_eq!(Language::from_extension(".lua"), Some(Language::Lua));
        assert_eq!(Language::from_extension(".v"), Some(Language::Verilog));
        assert_eq!(Language::from_extension(".sv"), Some(Language::Verilog));
        assert_eq!(Language::from_extension(".hs"), Some(Language::Haskell));
        assert_eq!(Language::from_extension(".lhs"), Some(Language::Haskell));
        assert_eq!(Language::from_extension(".ex"), Some(Language::Elixir));
        assert_eq!(Language::from_extension(".exs"), Some(Language::Elixir));
        assert_eq!(Language::from_extension(".groovy"), Some(Language::Groovy));
        assert_eq!(Language::from_extension(".gradle"), Some(Language::Groovy));
        assert_eq!(Language::from_extension(".ps1"), Some(Language::PowerShell));
        assert_eq!(
            Language::from_extension(".psm1"),
            Some(Language::PowerShell)
        );
        assert_eq!(Language::from_extension(".clj"), Some(Language::Clojure));
        assert_eq!(Language::from_extension(".cljs"), Some(Language::Clojure));
        assert_eq!(Language::from_extension(".jl"), Some(Language::Julia));
        assert_eq!(Language::from_extension(".r"), Some(Language::R));
        assert_eq!(Language::from_extension(".R"), Some(Language::R));
        assert_eq!(Language::from_extension(".erl"), Some(Language::Erlang));
        assert_eq!(Language::from_extension(".hrl"), Some(Language::Erlang));
        assert_eq!(Language::from_extension(".elm"), Some(Language::Elm));
        assert_eq!(Language::from_extension(".f90"), Some(Language::Fortran));
        assert_eq!(Language::from_extension(".f95"), Some(Language::Fortran));
        assert_eq!(Language::from_extension(".nix"), Some(Language::Nix));
        // Unsupported
        assert_eq!(Language::from_extension(".yaml"), None);
    }

    #[test]
    fn test_node_kind_roundtrip() {
        for kind in [
            NodeKind::Function,
            NodeKind::Class,
            NodeKind::Method,
            NodeKind::Interface,
            NodeKind::TypeAlias,
            NodeKind::Enum,
            NodeKind::Variable,
            NodeKind::Struct,
            NodeKind::Trait,
            NodeKind::Module,
            NodeKind::Property,
            NodeKind::Namespace,
            NodeKind::Constant,
        ] {
            let s = kind.as_str();
            assert_eq!(NodeKind::from_str_loose(s), Some(kind));
        }
    }

    #[test]
    fn test_edge_kind_roundtrip() {
        for kind in [
            EdgeKind::Imports,
            EdgeKind::Calls,
            EdgeKind::Contains,
            EdgeKind::Extends,
            EdgeKind::Implements,
            EdgeKind::References,
        ] {
            let s = kind.as_str();
            assert_eq!(EdgeKind::from_str_loose(s), Some(kind));
        }
    }

    #[test]
    fn test_language_query_source_not_empty() {
        for lang in ALL_LANGUAGES {
            assert!(!lang.query_source().is_empty(), "{} query is empty", lang);
        }
    }

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
        // Phase 11
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

    #[test]
    fn test_language_as_str_from_str_roundtrip() {
        for lang in ALL_LANGUAGES {
            let s = lang.as_str();
            assert!(
                Language::from_str_loose(s).is_some(),
                "from_str_loose({}) returned None for {:?}",
                s,
                lang
            );
            assert_eq!(
                Language::from_str_loose(s).unwrap(),
                lang,
                "roundtrip failed for {:?}",
                lang
            );
        }
    }

    #[test]
    fn test_language_from_str_loose_aliases() {
        // Test various aliases for new languages
        assert_eq!(Language::from_str_loose("shell"), Some(Language::Bash));
        assert_eq!(Language::from_str_loose("sh"), Some(Language::Bash));
        assert_eq!(Language::from_str_loose("hs"), Some(Language::Haskell));
        assert_eq!(Language::from_str_loose("ex"), Some(Language::Elixir));
        assert_eq!(Language::from_str_loose("clj"), Some(Language::Clojure));
        assert_eq!(Language::from_str_loose("jl"), Some(Language::Julia));
        assert_eq!(Language::from_str_loose("erl"), Some(Language::Erlang));
        assert_eq!(Language::from_str_loose("ps1"), Some(Language::PowerShell));
        assert_eq!(Language::from_str_loose("f90"), Some(Language::Fortran));
        assert_eq!(
            Language::from_str_loose("systemverilog"),
            Some(Language::Verilog)
        );
        assert_eq!(Language::from_str_loose("sv"), Some(Language::Verilog));
    }

    #[test]
    fn test_language_grammar_name_not_empty() {
        for lang in ALL_LANGUAGES {
            assert!(
                !lang.grammar_name().is_empty(),
                "{} grammar name is empty",
                lang
            );
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let node = CodeNode {
            id: "function:src/main.ts:hello:10".to_string(),
            name: "hello".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "src/main.ts".to_string(),
            start_line: 10,
            end_line: 15,
            start_column: 0,
            end_column: 1,
            language: Language::TypeScript,
            body: Some("function hello() {}".to_string()),
            documentation: None,
            exported: Some(true),
        };

        let json = serde_json::to_string(&node).unwrap();
        let back: CodeNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, node.id);
        assert_eq!(back.name, node.name);
        assert_eq!(back.qualified_name, None);
    }

    // =====================================================================
    // Parameterized Language::from_extension tests (test-case)
    // =====================================================================

    use test_case::test_case;

    // -- TypeScript family --
    #[test_case(".ts", Language::TypeScript ; "ext_ts")]
    #[test_case(".tsx", Language::Tsx ; "ext_tsx")]
    // -- JavaScript family --
    #[test_case(".js", Language::JavaScript ; "ext_js")]
    #[test_case(".mjs", Language::JavaScript ; "ext_mjs")]
    #[test_case(".cjs", Language::JavaScript ; "ext_cjs")]
    #[test_case(".jsx", Language::Jsx ; "ext_jsx")]
    // -- Python --
    #[test_case(".py", Language::Python ; "ext_py")]
    // -- Go --
    #[test_case(".go", Language::Go ; "ext_go")]
    // -- Rust --
    #[test_case(".rs", Language::Rust ; "ext_rs")]
    // -- Java --
    #[test_case(".java", Language::Java ; "ext_java")]
    // -- C family --
    #[test_case(".c", Language::C ; "ext_c")]
    #[test_case(".h", Language::C ; "ext_h")]
    #[test_case(".cpp", Language::Cpp ; "ext_cpp")]
    #[test_case(".cc", Language::Cpp ; "ext_cc")]
    #[test_case(".cxx", Language::Cpp ; "ext_cxx")]
    #[test_case(".hpp", Language::Cpp ; "ext_hpp")]
    #[test_case(".hxx", Language::Cpp ; "ext_hxx")]
    #[test_case(".hh", Language::Cpp ; "ext_hh")]
    // -- C# --
    #[test_case(".cs", Language::CSharp ; "ext_cs")]
    // -- PHP --
    #[test_case(".php", Language::Php ; "ext_php")]
    // -- Ruby --
    #[test_case(".rb", Language::Ruby ; "ext_rb")]
    // -- Swift --
    #[test_case(".swift", Language::Swift ; "ext_swift")]
    // -- Kotlin --
    #[test_case(".kt", Language::Kotlin ; "ext_kt")]
    #[test_case(".kts", Language::Kotlin ; "ext_kts")]
    // -- Bash --
    #[test_case(".sh", Language::Bash ; "ext_sh")]
    #[test_case(".bash", Language::Bash ; "ext_bash")]
    #[test_case(".zsh", Language::Bash ; "ext_zsh")]
    // -- Scala --
    #[test_case(".scala", Language::Scala ; "ext_scala")]
    #[test_case(".sc", Language::Scala ; "ext_sc")]
    // -- Dart --
    #[test_case(".dart", Language::Dart ; "ext_dart")]
    // -- Zig --
    #[test_case(".zig", Language::Zig ; "ext_zig")]
    // -- Lua --
    #[test_case(".lua", Language::Lua ; "ext_lua")]
    // -- Verilog --
    #[test_case(".v", Language::Verilog ; "ext_v")]
    #[test_case(".vh", Language::Verilog ; "ext_vh")]
    #[test_case(".sv", Language::Verilog ; "ext_sv")]
    #[test_case(".svh", Language::Verilog ; "ext_svh")]
    // -- Haskell --
    #[test_case(".hs", Language::Haskell ; "ext_hs")]
    #[test_case(".lhs", Language::Haskell ; "ext_lhs")]
    // -- Elixir --
    #[test_case(".ex", Language::Elixir ; "ext_ex")]
    #[test_case(".exs", Language::Elixir ; "ext_exs")]
    // -- Groovy --
    #[test_case(".groovy", Language::Groovy ; "ext_groovy")]
    #[test_case(".gradle", Language::Groovy ; "ext_gradle")]
    // -- PowerShell --
    #[test_case(".ps1", Language::PowerShell ; "ext_ps1")]
    #[test_case(".psm1", Language::PowerShell ; "ext_psm1")]
    #[test_case(".psd1", Language::PowerShell ; "ext_psd1")]
    // -- Clojure --
    #[test_case(".clj", Language::Clojure ; "ext_clj")]
    #[test_case(".cljs", Language::Clojure ; "ext_cljs")]
    #[test_case(".cljc", Language::Clojure ; "ext_cljc")]
    #[test_case(".edn", Language::Clojure ; "ext_edn")]
    // -- Julia --
    #[test_case(".jl", Language::Julia ; "ext_jl")]
    // -- R --
    #[test_case(".r", Language::R ; "ext_r_lower")]
    #[test_case(".R", Language::R ; "ext_r_upper")]
    #[test_case(".Rmd", Language::R ; "ext_rmd")]
    // -- Erlang --
    #[test_case(".erl", Language::Erlang ; "ext_erl")]
    #[test_case(".hrl", Language::Erlang ; "ext_hrl")]
    // -- Elm --
    #[test_case(".elm", Language::Elm ; "ext_elm")]
    // -- Fortran --
    #[test_case(".f90", Language::Fortran ; "ext_f90")]
    #[test_case(".f95", Language::Fortran ; "ext_f95")]
    #[test_case(".f03", Language::Fortran ; "ext_f03")]
    #[test_case(".f08", Language::Fortran ; "ext_f08")]
    #[test_case(".f", Language::Fortran ; "ext_f")]
    #[test_case(".for", Language::Fortran ; "ext_for")]
    #[test_case(".fpp", Language::Fortran ; "ext_fpp")]
    // -- Nix --
    #[test_case(".nix", Language::Nix ; "ext_nix")]
    fn from_extension_maps_correctly(ext: &str, expected: Language) {
        assert_eq!(Language::from_extension(ext), Some(expected));
    }

    // -- Unknown extensions return None --
    #[test_case(".yaml" ; "unknown_yaml")]
    #[test_case(".json" ; "unknown_json")]
    #[test_case(".toml" ; "unknown_toml")]
    #[test_case(".xml" ; "unknown_xml")]
    #[test_case(".md" ; "unknown_md")]
    #[test_case(".txt" ; "unknown_txt")]
    #[test_case(".html" ; "unknown_html")]
    #[test_case(".css" ; "unknown_css")]
    #[test_case(".svg" ; "unknown_svg")]
    #[test_case(".png" ; "unknown_png")]
    #[test_case(".wasm" ; "unknown_wasm")]
    #[test_case(".sql" ; "unknown_sql")]
    #[test_case("" ; "unknown_empty")]
    #[test_case("." ; "unknown_dot")]
    #[test_case(".123" ; "unknown_numeric")]
    fn from_extension_returns_none_for_unknown(ext: &str) {
        assert_eq!(Language::from_extension(ext), None);
    }

    // =====================================================================
    // Language::as_str() parameterized tests
    // =====================================================================

    #[test_case(Language::TypeScript, "typescript" ; "as_str_ts")]
    #[test_case(Language::Tsx, "tsx" ; "as_str_tsx")]
    #[test_case(Language::JavaScript, "javascript" ; "as_str_js")]
    #[test_case(Language::Jsx, "jsx" ; "as_str_jsx")]
    #[test_case(Language::Python, "python" ; "as_str_python")]
    #[test_case(Language::Go, "go" ; "as_str_go")]
    #[test_case(Language::Rust, "rust" ; "as_str_rust")]
    #[test_case(Language::Java, "java" ; "as_str_java")]
    #[test_case(Language::C, "c" ; "as_str_c")]
    #[test_case(Language::Cpp, "cpp" ; "as_str_cpp")]
    #[test_case(Language::CSharp, "csharp" ; "as_str_csharp")]
    #[test_case(Language::Php, "php" ; "as_str_php")]
    #[test_case(Language::Ruby, "ruby" ; "as_str_ruby")]
    #[test_case(Language::Swift, "swift" ; "as_str_swift")]
    #[test_case(Language::Kotlin, "kotlin" ; "as_str_kotlin")]
    #[test_case(Language::Bash, "bash" ; "as_str_bash")]
    #[test_case(Language::Scala, "scala" ; "as_str_scala")]
    #[test_case(Language::Dart, "dart" ; "as_str_dart")]
    #[test_case(Language::Zig, "zig" ; "as_str_zig")]
    #[test_case(Language::Lua, "lua" ; "as_str_lua")]
    #[test_case(Language::Verilog, "verilog" ; "as_str_verilog")]
    #[test_case(Language::Haskell, "haskell" ; "as_str_haskell")]
    #[test_case(Language::Elixir, "elixir" ; "as_str_elixir")]
    #[test_case(Language::Groovy, "groovy" ; "as_str_groovy")]
    #[test_case(Language::PowerShell, "powershell" ; "as_str_powershell")]
    #[test_case(Language::Clojure, "clojure" ; "as_str_clojure")]
    #[test_case(Language::Julia, "julia" ; "as_str_julia")]
    #[test_case(Language::R, "r" ; "as_str_r")]
    #[test_case(Language::Erlang, "erlang" ; "as_str_erlang")]
    #[test_case(Language::Elm, "elm" ; "as_str_elm")]
    #[test_case(Language::Fortran, "fortran" ; "as_str_fortran")]
    #[test_case(Language::Nix, "nix" ; "as_str_nix")]
    fn as_str_returns_expected(lang: Language, expected: &str) {
        assert_eq!(lang.as_str(), expected);
    }

    // =====================================================================
    // Language::from_str_loose() parameterized tests with aliases
    // =====================================================================

    #[test_case("typescript", Language::TypeScript ; "loose_typescript")]
    #[test_case("tsx", Language::Tsx ; "loose_tsx")]
    #[test_case("javascript", Language::JavaScript ; "loose_javascript")]
    #[test_case("jsx", Language::Jsx ; "loose_jsx")]
    #[test_case("python", Language::Python ; "loose_python")]
    #[test_case("go", Language::Go ; "loose_go")]
    #[test_case("golang", Language::Go ; "loose_golang")]
    #[test_case("rust", Language::Rust ; "loose_rust")]
    #[test_case("java", Language::Java ; "loose_java")]
    #[test_case("c", Language::C ; "loose_c")]
    #[test_case("cpp", Language::Cpp ; "loose_cpp")]
    #[test_case("c++", Language::Cpp ; "loose_cplusplus")]
    #[test_case("csharp", Language::CSharp ; "loose_csharp")]
    #[test_case("c#", Language::CSharp ; "loose_csharp_hash")]
    #[test_case("c_sharp", Language::CSharp ; "loose_c_sharp")]
    #[test_case("php", Language::Php ; "loose_php")]
    #[test_case("ruby", Language::Ruby ; "loose_ruby")]
    #[test_case("swift", Language::Swift ; "loose_swift")]
    #[test_case("kotlin", Language::Kotlin ; "loose_kotlin")]
    #[test_case("bash", Language::Bash ; "loose_bash")]
    #[test_case("shell", Language::Bash ; "loose_shell")]
    #[test_case("sh", Language::Bash ; "loose_sh")]
    #[test_case("scala", Language::Scala ; "loose_scala")]
    #[test_case("dart", Language::Dart ; "loose_dart")]
    #[test_case("zig", Language::Zig ; "loose_zig")]
    #[test_case("lua", Language::Lua ; "loose_lua")]
    #[test_case("verilog", Language::Verilog ; "loose_verilog")]
    #[test_case("systemverilog", Language::Verilog ; "loose_systemverilog")]
    #[test_case("sv", Language::Verilog ; "loose_sv")]
    #[test_case("haskell", Language::Haskell ; "loose_haskell")]
    #[test_case("hs", Language::Haskell ; "loose_hs")]
    #[test_case("elixir", Language::Elixir ; "loose_elixir")]
    #[test_case("ex", Language::Elixir ; "loose_ex")]
    #[test_case("groovy", Language::Groovy ; "loose_groovy")]
    #[test_case("powershell", Language::PowerShell ; "loose_powershell")]
    #[test_case("ps1", Language::PowerShell ; "loose_ps1")]
    #[test_case("clojure", Language::Clojure ; "loose_clojure")]
    #[test_case("clj", Language::Clojure ; "loose_clj")]
    #[test_case("julia", Language::Julia ; "loose_julia")]
    #[test_case("jl", Language::Julia ; "loose_jl")]
    #[test_case("r", Language::R ; "loose_r")]
    #[test_case("erlang", Language::Erlang ; "loose_erlang")]
    #[test_case("erl", Language::Erlang ; "loose_erl")]
    #[test_case("elm", Language::Elm ; "loose_elm")]
    #[test_case("fortran", Language::Fortran ; "loose_fortran")]
    #[test_case("f90", Language::Fortran ; "loose_f90")]
    #[test_case("nix", Language::Nix ; "loose_nix")]
    fn from_str_loose_resolves(input: &str, expected: Language) {
        assert_eq!(Language::from_str_loose(input), Some(expected));
    }

    // -- Case insensitivity for from_str_loose --
    #[test_case("TYPESCRIPT" ; "case_upper_typescript")]
    #[test_case("TypeScript" ; "case_mixed_typescript")]
    #[test_case("PYTHON" ; "case_upper_python")]
    #[test_case("GoLang" ; "case_mixed_golang")]
    #[test_case("BASH" ; "case_upper_bash")]
    #[test_case("Haskell" ; "case_mixed_haskell")]
    #[test_case("CLOJURE" ; "case_upper_clojure")]
    fn from_str_loose_is_case_insensitive(input: &str) {
        assert!(Language::from_str_loose(input).is_some());
    }

    // -- from_str_loose returns None for unknown --
    #[test_case("perl" ; "loose_unknown_perl")]
    #[test_case("cobol" ; "loose_unknown_cobol")]
    #[test_case("" ; "loose_unknown_empty")]
    #[test_case("xyz" ; "loose_unknown_xyz")]
    fn from_str_loose_returns_none_for_unknown(input: &str) {
        assert_eq!(Language::from_str_loose(input), None);
    }

    // =====================================================================
    // Language::grammar_name() parameterized
    // =====================================================================

    #[test_case(Language::TypeScript, "typescript" ; "grammar_ts")]
    #[test_case(Language::Tsx, "tsx" ; "grammar_tsx")]
    #[test_case(Language::JavaScript, "javascript" ; "grammar_js")]
    #[test_case(Language::Jsx, "javascript" ; "grammar_jsx")]
    #[test_case(Language::Python, "python" ; "grammar_python")]
    #[test_case(Language::Go, "go" ; "grammar_go")]
    #[test_case(Language::Rust, "rust" ; "grammar_rust")]
    #[test_case(Language::Java, "java" ; "grammar_java")]
    #[test_case(Language::C, "c" ; "grammar_c")]
    #[test_case(Language::Cpp, "cpp" ; "grammar_cpp")]
    #[test_case(Language::CSharp, "c_sharp" ; "grammar_csharp")]
    #[test_case(Language::Php, "php" ; "grammar_php")]
    #[test_case(Language::Ruby, "ruby" ; "grammar_ruby")]
    #[test_case(Language::Swift, "swift" ; "grammar_swift")]
    #[test_case(Language::Kotlin, "kotlin" ; "grammar_kotlin")]
    #[test_case(Language::Bash, "bash" ; "grammar_bash")]
    #[test_case(Language::Scala, "scala" ; "grammar_scala")]
    #[test_case(Language::Dart, "dart" ; "grammar_dart")]
    #[test_case(Language::Zig, "zig" ; "grammar_zig")]
    #[test_case(Language::Lua, "lua" ; "grammar_lua")]
    #[test_case(Language::Verilog, "verilog" ; "grammar_verilog")]
    #[test_case(Language::Haskell, "haskell" ; "grammar_haskell")]
    #[test_case(Language::Elixir, "elixir" ; "grammar_elixir")]
    #[test_case(Language::Groovy, "groovy" ; "grammar_groovy")]
    #[test_case(Language::PowerShell, "powershell" ; "grammar_powershell")]
    #[test_case(Language::Clojure, "clojure" ; "grammar_clojure")]
    #[test_case(Language::Julia, "julia" ; "grammar_julia")]
    #[test_case(Language::R, "r" ; "grammar_r")]
    #[test_case(Language::Erlang, "erlang" ; "grammar_erlang")]
    #[test_case(Language::Elm, "elm" ; "grammar_elm")]
    #[test_case(Language::Fortran, "fortran" ; "grammar_fortran")]
    #[test_case(Language::Nix, "nix" ; "grammar_nix")]
    fn grammar_name_returns_expected(lang: Language, expected: &str) {
        assert_eq!(lang.grammar_name(), expected);
    }

    // =====================================================================
    // Language::Display trait
    // =====================================================================

    #[test]
    fn display_matches_as_str_for_all_languages() {
        for lang in ALL_LANGUAGES {
            assert_eq!(format!("{lang}"), lang.as_str());
        }
    }

    // =====================================================================
    // NodeKind parameterized tests
    // =====================================================================

    #[test_case(NodeKind::Function, "function" ; "nk_function")]
    #[test_case(NodeKind::Class, "class" ; "nk_class")]
    #[test_case(NodeKind::Method, "method" ; "nk_method")]
    #[test_case(NodeKind::Interface, "interface" ; "nk_interface")]
    #[test_case(NodeKind::TypeAlias, "type_alias" ; "nk_type_alias")]
    #[test_case(NodeKind::Enum, "enum" ; "nk_enum")]
    #[test_case(NodeKind::Variable, "variable" ; "nk_variable")]
    #[test_case(NodeKind::Struct, "struct" ; "nk_struct")]
    #[test_case(NodeKind::Trait, "trait" ; "nk_trait")]
    #[test_case(NodeKind::Module, "module" ; "nk_module")]
    #[test_case(NodeKind::Property, "property" ; "nk_property")]
    #[test_case(NodeKind::Namespace, "namespace" ; "nk_namespace")]
    #[test_case(NodeKind::Constant, "constant" ; "nk_constant")]
    fn node_kind_as_str_expected(kind: NodeKind, expected: &str) {
        assert_eq!(kind.as_str(), expected);
    }

    // -- NodeKind::from_str_loose with aliases --
    #[test_case("field", NodeKind::Property ; "nk_loose_field")]
    #[test_case("package", NodeKind::Namespace ; "nk_loose_package")]
    #[test_case("const", NodeKind::Constant ; "nk_loose_const")]
    fn node_kind_from_str_loose_aliases(input: &str, expected: NodeKind) {
        assert_eq!(NodeKind::from_str_loose(input), Some(expected));
    }

    #[test_case("unknown" ; "nk_loose_unknown")]
    #[test_case("" ; "nk_loose_empty")]
    #[test_case("func" ; "nk_loose_func")]
    fn node_kind_from_str_loose_returns_none(input: &str) {
        assert_eq!(NodeKind::from_str_loose(input), None);
    }

    #[test]
    fn node_kind_display_matches_as_str() {
        let kinds = [
            NodeKind::Function,
            NodeKind::Class,
            NodeKind::Method,
            NodeKind::Interface,
            NodeKind::TypeAlias,
            NodeKind::Enum,
            NodeKind::Variable,
            NodeKind::Struct,
            NodeKind::Trait,
            NodeKind::Module,
            NodeKind::Property,
            NodeKind::Namespace,
            NodeKind::Constant,
        ];
        for kind in kinds {
            assert_eq!(format!("{kind}"), kind.as_str());
        }
    }

    // =====================================================================
    // EdgeKind parameterized tests
    // =====================================================================

    #[test_case(EdgeKind::Imports, "imports" ; "ek_imports")]
    #[test_case(EdgeKind::Calls, "calls" ; "ek_calls")]
    #[test_case(EdgeKind::Contains, "contains" ; "ek_contains")]
    #[test_case(EdgeKind::Extends, "extends" ; "ek_extends")]
    #[test_case(EdgeKind::Implements, "implements" ; "ek_implements")]
    #[test_case(EdgeKind::References, "references" ; "ek_references")]
    fn edge_kind_as_str_expected(kind: EdgeKind, expected: &str) {
        assert_eq!(kind.as_str(), expected);
    }

    #[test_case("nonexistent" ; "ek_loose_nonexistent")]
    #[test_case("" ; "ek_loose_empty")]
    #[test_case("call" ; "ek_loose_call_no_s")]
    fn edge_kind_from_str_loose_returns_none(input: &str) {
        assert_eq!(EdgeKind::from_str_loose(input), None);
    }

    #[test]
    fn edge_kind_display_matches_as_str() {
        let kinds = [
            EdgeKind::Imports,
            EdgeKind::Calls,
            EdgeKind::Contains,
            EdgeKind::Extends,
            EdgeKind::Implements,
            EdgeKind::References,
        ];
        for kind in kinds {
            assert_eq!(format!("{kind}"), kind.as_str());
        }
    }

    // =====================================================================
    // make_node_id() tests
    // =====================================================================

    #[test_case(NodeKind::Function, "src/main.ts", "hello", 10, "function:src/main.ts:hello:10" ; "id_function")]
    #[test_case(NodeKind::Class, "app.py", "UserService", 1, "class:app.py:UserService:1" ; "id_class")]
    #[test_case(NodeKind::Method, "lib.rs", "process", 42, "method:lib.rs:process:42" ; "id_method")]
    #[test_case(NodeKind::Variable, "index.js", "config", 0, "variable:index.js:config:0" ; "id_variable")]
    #[test_case(NodeKind::Constant, "constants.go", "MAX_SIZE", 5, "constant:constants.go:MAX_SIZE:5" ; "id_constant")]
    fn make_node_id_format(kind: NodeKind, file: &str, name: &str, line: u32, expected: &str) {
        assert_eq!(make_node_id(kind, file, name, line), expected);
    }

    #[test]
    fn make_node_id_with_special_characters() {
        let id = make_node_id(
            NodeKind::Function,
            "src/path with spaces/main.ts",
            "fn$name",
            1,
        );
        assert_eq!(id, "function:src/path with spaces/main.ts:fn$name:1");
    }

    // =====================================================================
    // CodeNode serde tests
    // =====================================================================

    #[test]
    fn code_node_serde_skips_none_fields() {
        let node = CodeNode {
            id: "test:id:1:1".to_string(),
            name: "test".to_string(),
            qualified_name: None,
            kind: NodeKind::Function,
            file_path: "test.rs".to_string(),
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 1,
            language: Language::Rust,
            body: None,
            documentation: None,
            exported: None,
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(!json.contains("qualified_name"));
        assert!(!json.contains("body"));
        assert!(!json.contains("documentation"));
        assert!(!json.contains("exported"));
    }

    #[test]
    fn code_node_serde_includes_some_fields() {
        let node = CodeNode {
            id: "test:id:1:1".to_string(),
            name: "test".to_string(),
            qualified_name: Some("MyClass.test".to_string()),
            kind: NodeKind::Method,
            file_path: "test.rs".to_string(),
            start_line: 1,
            end_line: 5,
            start_column: 0,
            end_column: 1,
            language: Language::Rust,
            body: Some("fn test() {}".to_string()),
            documentation: Some("/// A test method".to_string()),
            exported: Some(true),
        };

        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("qualified_name"));
        assert!(json.contains("MyClass.test"));
        assert!(json.contains("body"));
        assert!(json.contains("documentation"));
        assert!(json.contains("exported"));
    }

    #[test]
    fn code_node_serde_roundtrip_with_all_fields() {
        let node = CodeNode {
            id: "method:app.ts:MyClass.greet:10".to_string(),
            name: "greet".to_string(),
            qualified_name: Some("MyClass.greet".to_string()),
            kind: NodeKind::Method,
            file_path: "app.ts".to_string(),
            start_line: 10,
            end_line: 15,
            start_column: 4,
            end_column: 5,
            language: Language::TypeScript,
            body: Some("greet() { return 'hello'; }".to_string()),
            documentation: Some("Greeting method".to_string()),
            exported: Some(false),
        };

        let json = serde_json::to_string(&node).unwrap();
        let back: CodeNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, node.id);
        assert_eq!(back.name, node.name);
        assert_eq!(back.qualified_name, Some("MyClass.greet".to_string()));
        assert_eq!(back.kind, NodeKind::Method);
        assert_eq!(back.start_line, 10);
        assert_eq!(back.end_line, 15);
        assert_eq!(back.start_column, 4);
        assert_eq!(back.end_column, 5);
        assert_eq!(back.language, Language::TypeScript);
        assert_eq!(back.exported, Some(false));
    }

    // =====================================================================
    // CodeEdge serde tests
    // =====================================================================

    #[test]
    fn code_edge_serde_roundtrip() {
        let edge = CodeEdge {
            source: "function:a.ts:foo:1".to_string(),
            target: "function:b.ts:bar:5".to_string(),
            kind: EdgeKind::Calls,
            file_path: "a.ts".to_string(),
            line: 3,
            metadata: None,
        };

        let json = serde_json::to_string(&edge).unwrap();
        let back: CodeEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, edge.source);
        assert_eq!(back.target, edge.target);
        assert_eq!(back.kind, EdgeKind::Calls);
        assert_eq!(back.line, 3);
    }

    #[test]
    fn code_edge_serde_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("alias".to_string(), "myAlias".to_string());
        let edge = CodeEdge {
            source: "function:a.ts:foo:1".to_string(),
            target: "module:b.ts:b:0".to_string(),
            kind: EdgeKind::Imports,
            file_path: "a.ts".to_string(),
            line: 1,
            metadata: Some(metadata),
        };

        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("alias"));
        let back: CodeEdge = serde_json::from_str(&json).unwrap();
        assert!(back.metadata.is_some());
        assert_eq!(back.metadata.unwrap().get("alias").unwrap(), "myAlias");
    }

    #[test]
    fn code_edge_serde_skips_none_metadata() {
        let edge = CodeEdge {
            source: "s".to_string(),
            target: "t".to_string(),
            kind: EdgeKind::References,
            file_path: "f.rs".to_string(),
            line: 1,
            metadata: None,
        };

        let json = serde_json::to_string(&edge).unwrap();
        assert!(!json.contains("metadata"));
    }

    // =====================================================================
    // UnresolvedRef tests
    // =====================================================================

    #[test]
    fn unresolved_ref_serde_roundtrip() {
        let uref = UnresolvedRef {
            id: 42,
            source_id: "function:main.ts:app:1".to_string(),
            specifier: "./utils".to_string(),
            ref_type: "import".to_string(),
            file_path: "main.ts".to_string(),
            line: 3,
        };

        let json = serde_json::to_string(&uref).unwrap();
        let back: UnresolvedRef = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, 42);
        assert_eq!(back.source_id, "function:main.ts:app:1");
        assert_eq!(back.specifier, "./utils");
        assert_eq!(back.ref_type, "import");
        assert_eq!(back.line, 3);
    }

    // =====================================================================
    // Language serde tests
    // =====================================================================

    #[test]
    fn language_serde_all_variants() {
        for lang in ALL_LANGUAGES {
            let json = serde_json::to_string(&lang).unwrap();
            let back: Language = serde_json::from_str(&json).unwrap();
            assert_eq!(back, lang, "serde roundtrip failed for {:?}", lang);
        }
    }

    #[test]
    fn language_serde_uses_lowercase() {
        let json = serde_json::to_string(&Language::TypeScript).unwrap();
        assert_eq!(json, "\"typescript\"");
        let json = serde_json::to_string(&Language::CSharp).unwrap();
        assert_eq!(json, "\"csharp\"");
    }

    // =====================================================================
    // NodeKind serde tests
    // =====================================================================

    #[test]
    fn node_kind_serde_all_variants() {
        let kinds = [
            NodeKind::Function,
            NodeKind::Class,
            NodeKind::Method,
            NodeKind::Interface,
            NodeKind::TypeAlias,
            NodeKind::Enum,
            NodeKind::Variable,
            NodeKind::Struct,
            NodeKind::Trait,
            NodeKind::Module,
            NodeKind::Property,
            NodeKind::Namespace,
            NodeKind::Constant,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: NodeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind, "serde roundtrip failed for {:?}", kind);
        }
    }

    #[test]
    fn node_kind_serde_uses_snake_case() {
        let json = serde_json::to_string(&NodeKind::TypeAlias).unwrap();
        assert_eq!(json, "\"type_alias\"");
    }

    // =====================================================================
    // EdgeKind serde tests
    // =====================================================================

    #[test]
    fn edge_kind_serde_all_variants() {
        let kinds = [
            EdgeKind::Imports,
            EdgeKind::Calls,
            EdgeKind::Contains,
            EdgeKind::Extends,
            EdgeKind::Implements,
            EdgeKind::References,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: EdgeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind, "serde roundtrip failed for {:?}", kind);
        }
    }

    #[test]
    fn edge_kind_serde_uses_snake_case() {
        let json = serde_json::to_string(&EdgeKind::Imports).unwrap();
        assert_eq!(json, "\"imports\"");
    }

    // =====================================================================
    // Property-based tests
    // =====================================================================

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn language_from_extension_never_panics(s in "\\PC{1,10}") {
            let _ = Language::from_extension(&s);
        }

        #[test]
        fn language_from_str_loose_never_panics(s in "\\PC{0,50}") {
            let _ = Language::from_str_loose(&s);
        }

        #[test]
        fn node_kind_from_str_loose_never_panics(s in "\\PC{0,50}") {
            let _ = NodeKind::from_str_loose(&s);
        }

        #[test]
        fn edge_kind_from_str_loose_never_panics(s in "\\PC{0,50}") {
            let _ = EdgeKind::from_str_loose(&s);
        }

        #[test]
        fn make_node_id_never_panics(
            file in "\\PC{1,50}",
            name in "\\PC{1,30}",
            line in 0u32..100000u32
        ) {
            let id = make_node_id(NodeKind::Function, &file, &name, line);
            assert!(id.starts_with("function:"));
            assert!(id.contains(&name));
        }

        #[test]
        fn language_as_str_roundtrips_through_from_str_loose(idx in 0usize..32) {
            let lang = ALL_LANGUAGES[idx];
            let s = lang.as_str();
            assert_eq!(Language::from_str_loose(s), Some(lang));
        }
    }
}
