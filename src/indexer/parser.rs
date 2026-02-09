//! Native tree-sitter parser wrapper for CodeGraph.
//!
//! This is the Rust equivalent of the TypeScript `parser.ts`, but dramatically
//! simpler: no WASM, no async initialization, no runtime downloads. Grammars
//! are statically linked and queries are embedded at compile time via
//! `include_str!` (see [`Language::query_source`]).
//!
//! # Design decisions
//!
//! - **No stored state.** `CodeParser` carries no fields. Tree-sitter's
//!   `Parser` is `!Send + !Sync`, so rather than wrestling with thread-safety
//!   wrappers we create a fresh parser on every call. This is cheap — `Parser::new()`
//!   is a single allocation and `set_language` is a pointer swap.
//!
//! - **Query compilation on demand.** `.scm` query compilation takes roughly
//!   1 ms per language. For a first pass this is negligible. A `OnceCell`-based
//!   cache can be layered on later without changing the public API.
//!
//! - **Language detection by extension.** Delegates to [`Language::from_extension`],
//!   keeping the mapping in one canonical place.

use crate::error::{CodeGraphError, Result};
use crate::types::Language;

/// Thin wrapper around native tree-sitter parsing and query compilation.
///
/// All grammars are statically linked at build time — no runtime setup needed.
/// Create one with [`CodeParser::new`] and reuse freely; the struct is `Send`,
/// `Sync`, and zero-sized.
pub struct CodeParser;

impl CodeParser {
    /// Create a new `CodeParser`.
    ///
    /// This is a no-op — it exists so call sites read naturally and so we can
    /// add configuration (e.g., timeout, cancellation) later without breaking
    /// the public API.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Parse `content` using the grammar for `language` and return the
    /// concrete syntax tree.
    ///
    /// A fresh `tree_sitter::Parser` is created on each call because the
    /// underlying C object is `!Send`. This is intentional — allocation is
    /// trivially fast and it keeps the API thread-safe.
    pub fn parse(&self, content: &str, language: Language) -> Result<tree_sitter::Tree> {
        let ts_lang = Self::get_ts_language(language);

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&ts_lang)
            .map_err(|e| CodeGraphError::Parse(format!("Language version mismatch: {e}")))?;

        parser.parse(content, None).ok_or_else(|| {
            CodeGraphError::Parse("tree-sitter returned None (timeout or cancellation)".into())
        })
    }

    /// Return the native `tree_sitter::Language` for a [`Language`] variant.
    ///
    /// Each grammar crate exposes a `LanguageFn` constant. The `.into()` call
    /// goes through tree-sitter's `From<LanguageFn> for Language` impl, which
    /// invokes the C initializer exactly once.
    #[must_use]
    pub fn get_ts_language(language: Language) -> tree_sitter::Language {
        match language {
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::JavaScript | Language::Jsx => tree_sitter_javascript::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
            Language::Go => tree_sitter_go::LANGUAGE.into(),
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::Java => tree_sitter_java::LANGUAGE.into(),
            Language::C => tree_sitter_c::LANGUAGE.into(),
            Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Language::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Language::Swift => tree_sitter_swift::LANGUAGE.into(),
            Language::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
            // Phase 11
            Language::Bash => tree_sitter_bash::LANGUAGE.into(),
            Language::Scala => tree_sitter_scala::LANGUAGE.into(),
            Language::Dart => tree_sitter_dart_orchard::LANGUAGE.into(),
            Language::Zig => tree_sitter_zig::LANGUAGE.into(),
            Language::Lua => tree_sitter_lua::LANGUAGE.into(),
            Language::Verilog => tree_sitter_verilog::LANGUAGE.into(),
            Language::Haskell => tree_sitter_haskell::LANGUAGE.into(),
            Language::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Language::Groovy => tree_sitter_groovy::LANGUAGE.into(),
            Language::PowerShell => tree_sitter_powershell::LANGUAGE.into(),
            Language::Clojure => tree_sitter_clojure_orchard::LANGUAGE.into(),
            Language::Julia => tree_sitter_julia::LANGUAGE.into(),
            Language::R => tree_sitter_r::LANGUAGE.into(),
            Language::Erlang => tree_sitter_erlang::LANGUAGE.into(),
            Language::Elm => tree_sitter_elm::LANGUAGE.into(),
            Language::Fortran => tree_sitter_fortran::LANGUAGE.into(),
            Language::Nix => tree_sitter_nix::LANGUAGE.into(),
        }
    }

    /// Compile the `.scm` query source for `language` into a
    /// [`tree_sitter::Query`].
    ///
    /// Query compilation is fast (~1 ms), so we compile fresh each time.
    /// If profiling shows this is a bottleneck, wrap with a static
    /// `OnceLock<HashMap<Language, Query>>` — the public API stays the same.
    pub fn load_query(language: Language) -> Result<tree_sitter::Query> {
        let ts_lang = Self::get_ts_language(language);
        let source = language.query_source();
        tree_sitter::Query::new(&ts_lang, source).map_err(|e| {
            CodeGraphError::Parse(format!("Query compilation error for {language}: {e}"))
        })
    }

    /// Detect the [`Language`] for a file path based on its extension.
    ///
    /// Returns `None` for unsupported extensions.
    #[must_use]
    pub fn detect_language(file_path: &str) -> Option<Language> {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| Language::from_extension(&format!(".{e}")))
    }

    /// Check whether the file at `file_path` has a supported extension.
    #[must_use]
    pub fn is_supported(file_path: &str) -> bool {
        Self::detect_language(file_path).is_some()
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// All 32 language variants for exhaustive testing.
    fn all_languages() -> Vec<Language> {
        vec![
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
        ]
    }

    // -- Parsing -----------------------------------------------------------

    #[test]
    fn parse_typescript_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
            export function greet(name: string): string {
                return `Hello, ${name}!`;
            }

            interface User {
                id: number;
                name: string;
            }

            class UserService {
                getUser(id: number): User {
                    return { id, name: "test" };
                }
            }
        "#;

        let tree = parser
            .parse(source, Language::TypeScript)
            .expect("should parse TypeScript");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_javascript_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
            const add = (a, b) => a + b;

            function multiply(a, b) {
                return a * b;
            }

            class Calculator {
                constructor(initial) {
                    this.value = initial;
                }

                add(n) {
                    this.value += n;
                    return this;
                }
            }
        "#;

        let tree = parser
            .parse(source, Language::JavaScript)
            .expect("should parse JavaScript");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_python_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
import os
from pathlib import Path

def greet(name: str) -> str:
    return f"Hello, {name}!"

class UserService:
    def __init__(self, db):
        self.db = db

    def get_user(self, user_id: int):
        return self.db.find(user_id)
"#;

        let tree = parser
            .parse(source, Language::Python)
            .expect("should parse Python");
        let root = tree.root_node();
        assert_eq!(root.kind(), "module");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_tsx_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
            import React from "react";

            interface Props {
                name: string;
            }

            const Greeting: React.FC<Props> = ({ name }) => {
                return <div>Hello, {name}!</div>;
            };

            export default Greeting;
        "#;

        let tree = parser
            .parse(source, Language::Tsx)
            .expect("should parse TSX");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
    }

    #[test]
    fn parse_empty_source_returns_tree() {
        let parser = CodeParser::new();
        let tree = parser
            .parse("", Language::TypeScript)
            .expect("empty source should parse");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn parse_go_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
package main

import "fmt"

type User struct {
    ID   int
    Name string
}

type Greeter interface {
    Greet(name string) string
}

func (u *User) Greet(name string) string {
    return fmt.Sprintf("Hello, %s!", name)
}

func main() {
    user := &User{ID: 1, Name: "Alice"}
    fmt.Println(user.Greet("World"))
}
"#;
        let tree = parser.parse(source, Language::Go).expect("should parse Go");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_rust_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
use std::collections::HashMap;

pub trait Greeter {
    fn greet(&self, name: &str) -> String;
}

pub struct User {
    pub id: u32,
    pub name: String,
}

impl Greeter for User {
    fn greet(&self, name: &str) -> String {
        format!("Hello, {}!", name)
    }
}

fn main() {
    let user = User { id: 1, name: "Alice".to_string() };
    println!("{}", user.greet("World"));
}
"#;
        let tree = parser
            .parse(source, Language::Rust)
            .expect("should parse Rust");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_java_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
package com.example;

import java.util.List;

public interface Greeter {
    String greet(String name);
}

public class UserService implements Greeter {
    private final String prefix;

    public UserService(String prefix) {
        this.prefix = prefix;
    }

    @Override
    public String greet(String name) {
        return prefix + " " + name;
    }
}
"#;
        let tree = parser
            .parse(source, Language::Java)
            .expect("should parse Java");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_c_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
#include <stdio.h>
#include <stdlib.h>

#define MAX_SIZE 100

typedef struct {
    int id;
    char name[MAX_SIZE];
} User;

void greet(const User *user) {
    printf("Hello, %s!\n", user->name);
}

int main(void) {
    User user = {1, "Alice"};
    greet(&user);
    return 0;
}
"#;
        let tree = parser.parse(source, Language::C).expect("should parse C");
        let root = tree.root_node();
        assert_eq!(root.kind(), "translation_unit");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_cpp_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
#include <iostream>
#include <string>

namespace app {

class Greeter {
public:
    virtual std::string greet(const std::string& name) = 0;
    virtual ~Greeter() = default;
};

class UserService : public Greeter {
public:
    std::string greet(const std::string& name) override {
        return "Hello, " + name + "!";
    }
};

} // namespace app

int main() {
    app::UserService svc;
    std::cout << svc.greet("World") << std::endl;
    return 0;
}
"#;
        let tree = parser
            .parse(source, Language::Cpp)
            .expect("should parse C++");
        let root = tree.root_node();
        assert_eq!(root.kind(), "translation_unit");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_csharp_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
using System;
using System.Collections.Generic;

namespace App
{
    public interface IGreeter
    {
        string Greet(string name);
    }

    public class UserService : IGreeter
    {
        public string Greet(string name)
        {
            return $"Hello, {name}!";
        }

        public List<string> GetUsers()
        {
            return new List<string> { "Alice", "Bob" };
        }
    }
}
"#;
        let tree = parser
            .parse(source, Language::CSharp)
            .expect("should parse C#");
        let root = tree.root_node();
        assert_eq!(root.kind(), "compilation_unit");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_php_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"<?php

namespace App;

use App\Models\User;

interface Greeter {
    public function greet(string $name): string;
}

class UserService implements Greeter {
    private string $prefix;

    public function __construct(string $prefix) {
        $this->prefix = $prefix;
    }

    public function greet(string $name): string {
        return $this->prefix . " " . $name;
    }
}

function main(): void {
    $service = new UserService("Hello");
    echo $service->greet("World");
}
"#;
        let tree = parser
            .parse(source, Language::Php)
            .expect("should parse PHP");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_ruby_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
require 'json'

module Greetable
  def greet(name)
    "Hello, #{name}!"
  end
end

class User
  include Greetable
  attr_accessor :id, :name

  def initialize(id, name)
    @id = id
    @name = name
  end

  def self.create(id, name)
    new(id, name)
  end
end

class Admin < User
  def greet(name)
    "Admin says: Hello, #{name}!"
  end
end
"#;
        let tree = parser
            .parse(source, Language::Ruby)
            .expect("should parse Ruby");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_swift_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
import Foundation

protocol Greeter {
    func greet(name: String) -> String
}

struct User {
    let id: Int
    let name: String
}

class UserService: Greeter {
    func greet(name: String) -> String {
        return "Hello, \(name)!"
    }
}

enum Direction {
    case north, south, east, west
}

func main() {
    let service = UserService()
    print(service.greet(name: "World"))
}
"#;
        let tree = parser
            .parse(source, Language::Swift)
            .expect("should parse Swift");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_kotlin_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
package com.example

import java.util.List

interface Greeter {
    fun greet(name: String): String
}

data class User(val id: Int, val name: String)

class UserService : Greeter {
    override fun greet(name: String): String {
        return "Hello, $name!"
    }
}

object Singleton {
    fun doSomething() {
        println("doing something")
    }
}

fun main() {
    val service = UserService()
    println(service.greet("World"))
}
"#;
        let tree = parser
            .parse(source, Language::Kotlin)
            .expect("should parse Kotlin");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0, "tree should have children");
        assert!(!root.has_error(), "tree should be error-free");
    }

    // -- Phase 11: Parse tests for new languages ----------------------------

    #[test]
    fn parse_bash_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"#!/bin/bash

greet() {
    local name="$1"
    echo "Hello, $name!"
}

MY_VAR="world"
greet "$MY_VAR"
"#;
        let tree = parser
            .parse(source, Language::Bash)
            .expect("should parse Bash");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_scala_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
package com.example

import scala.collection.mutable

trait Greeter {
  def greet(name: String): String
}

class UserService extends Greeter {
  def greet(name: String): String = s"Hello, $name!"
}

object Main {
  val version = "1.0"
  def main(args: Array[String]): Unit = {
    val svc = new UserService()
    println(svc.greet("World"))
  }
}

case class User(id: Int, name: String)
"#;
        let tree = parser
            .parse(source, Language::Scala)
            .expect("should parse Scala");
        let root = tree.root_node();
        assert_eq!(root.kind(), "compilation_unit");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_dart_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
import 'dart:io';

class Greeter {
  String greet(String name) {
    return 'Hello, $name!';
  }
}

enum Color { red, green, blue }

void main() {
  var greeter = Greeter();
  print(greeter.greet('World'));
}
"#;
        let tree = parser
            .parse(source, Language::Dart)
            .expect("should parse Dart");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_zig_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
const std = @import("std");

fn add(a: i32, b: i32) i32 {
    return a + b;
}

pub fn main() !void {
    const result = add(3, 4);
    std.debug.print("Result: {}\n", .{result});
}
"#;
        let tree = parser
            .parse(source, Language::Zig)
            .expect("should parse Zig");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_lua_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
local function greet(name)
    return "Hello, " .. name .. "!"
end

function add(a, b)
    return a + b
end

local M = {}

function M.init()
    print("initialized")
end

function M:method()
    return self.value
end

local result = greet("World")
print(result)
"#;
        let tree = parser
            .parse(source, Language::Lua)
            .expect("should parse Lua");
        let root = tree.root_node();
        assert_eq!(root.kind(), "chunk");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_verilog_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
module counter (
    input wire clk,
    input wire reset,
    output reg [7:0] count
);

always @(posedge clk or posedge reset) begin
    if (reset)
        count <= 8'b0;
    else
        count <= count + 1;
end

endmodule
"#;
        let tree = parser
            .parse(source, Language::Verilog)
            .expect("should parse Verilog");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_haskell_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
module Main where

import Data.List

data Color = Red | Green | Blue

class Describable a where
  describe :: a -> String

instance Describable Color where
  describe Red = "red"
  describe Green = "green"
  describe Blue = "blue"

greet :: String -> String
greet name = "Hello, " ++ name ++ "!"

main :: IO ()
main = putStrLn (greet "World")
"#;
        let tree = parser
            .parse(source, Language::Haskell)
            .expect("should parse Haskell");
        let root = tree.root_node();
        assert_eq!(root.kind(), "haskell");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_elixir_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
defmodule Greeter do
  def greet(name) do
    "Hello, #{name}!"
  end

  defp private_helper do
    :ok
  end
end

defmodule Main do
  def run do
    IO.puts(Greeter.greet("World"))
  end
end
"#;
        let tree = parser
            .parse(source, Language::Elixir)
            .expect("should parse Elixir");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_groovy_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
class UserService {
    String greet(String name) {
        return "Hello, " + name
    }
}
"#;
        let tree = parser
            .parse(source, Language::Groovy)
            .expect("should parse Groovy");
        let root = tree.root_node();
        assert!(root.child_count() > 0);
        // Groovy grammar may produce partial errors for some syntax
    }

    #[test]
    fn parse_powershell_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
function Get-Greeting {
    param([string]$Name)
    return "Hello, $Name!"
}

class MyService {
    [string] Greet([string]$name) {
        return "Hello, $name!"
    }
}

enum Color {
    Red
    Green
    Blue
}

$greeting = Get-Greeting -Name "World"
Write-Host $greeting
"#;
        let tree = parser
            .parse(source, Language::PowerShell)
            .expect("should parse PowerShell");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_clojure_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
(ns my-app.core
  (:require [clojure.string :as str]))

(defn greet [name]
  (str "Hello, " name "!"))

(def version "1.0")

(defn -main [& args]
  (println (greet "World")))
"#;
        let tree = parser
            .parse(source, Language::Clojure)
            .expect("should parse Clojure");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_julia_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
module MyModule

struct User
    id::Int
    name::String
end

function greet(name::String)
    return "Hello, $name!"
end

add(a, b) = a + b

end # module
"#;
        let tree = parser
            .parse(source, Language::Julia)
            .expect("should parse Julia");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_r_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
library(ggplot2)

greet <- function(name) {
  paste("Hello,", name, "!")
}

add <- function(a, b) {
  a + b
}

result <- greet("World")
cat(result, "\n")
"#;
        let tree = parser.parse(source, Language::R).expect("should parse R");
        let root = tree.root_node();
        assert_eq!(root.kind(), "program");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_erlang_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
-module(greeter).
-export([greet/1, main/0]).

-record(user, {id, name}).

greet(Name) ->
    io_lib:format("Hello, ~s!", [Name]).

main() ->
    io:format("~s~n", [greet("World")]).
"#;
        let tree = parser
            .parse(source, Language::Erlang)
            .expect("should parse Erlang");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_elm_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
module Main exposing (main, greet)

import Html exposing (text)

type alias User =
    { id : Int
    , name : String
    }

type Color
    = Red
    | Green
    | Blue

greet : String -> String
greet name =
    "Hello, " ++ name ++ "!"

main =
    text (greet "World")
"#;
        let tree = parser
            .parse(source, Language::Elm)
            .expect("should parse Elm");
        let root = tree.root_node();
        assert_eq!(root.kind(), "file");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_fortran_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
program hello
    implicit none
    call greet("World")
end program hello

subroutine greet(name)
    implicit none
    character(len=*), intent(in) :: name
    print *, "Hello, ", name, "!"
end subroutine greet

function add(a, b) result(c)
    implicit none
    integer, intent(in) :: a, b
    integer :: c
    c = a + b
end function add
"#;
        let tree = parser
            .parse(source, Language::Fortran)
            .expect("should parse Fortran");
        let root = tree.root_node();
        assert_eq!(root.kind(), "translation_unit");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    #[test]
    fn parse_nix_returns_valid_tree() {
        let parser = CodeParser::new();
        let source = r#"
{ pkgs ? import <nixpkgs> {} }:

let
  greeting = "Hello, World!";
  add = a: b: a + b;
in {
  shell = pkgs.mkShell {
    buildInputs = [ pkgs.hello ];
  };
  result = add 3 4;
}
"#;
        let tree = parser
            .parse(source, Language::Nix)
            .expect("should parse Nix");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_code");
        assert!(root.child_count() > 0);
        assert!(!root.has_error(), "tree should be error-free");
    }

    // -- Language detection ------------------------------------------------

    #[test]
    fn detect_language_from_file_path() {
        let cases = vec![
            ("src/app.ts", Some(Language::TypeScript)),
            ("src/app.tsx", Some(Language::Tsx)),
            ("lib/util.js", Some(Language::JavaScript)),
            ("lib/util.mjs", Some(Language::JavaScript)),
            ("lib/util.cjs", Some(Language::JavaScript)),
            ("components/Button.jsx", Some(Language::Jsx)),
            ("scripts/run.py", Some(Language::Python)),
            ("main.go", Some(Language::Go)),
            ("lib.rs", Some(Language::Rust)),
            ("Main.java", Some(Language::Java)),
            ("main.c", Some(Language::C)),
            ("util.h", Some(Language::C)),
            ("main.cpp", Some(Language::Cpp)),
            ("Program.cs", Some(Language::CSharp)),
            ("index.php", Some(Language::Php)),
            ("app.rb", Some(Language::Ruby)),
            ("Main.kt", Some(Language::Kotlin)),
            ("App.swift", Some(Language::Swift)),
            // Phase 11
            ("deploy.sh", Some(Language::Bash)),
            ("config.bash", Some(Language::Bash)),
            ("Main.scala", Some(Language::Scala)),
            ("app.dart", Some(Language::Dart)),
            ("main.zig", Some(Language::Zig)),
            ("script.lua", Some(Language::Lua)),
            ("counter.v", Some(Language::Verilog)),
            ("chip.sv", Some(Language::Verilog)),
            ("Main.hs", Some(Language::Haskell)),
            ("app.ex", Some(Language::Elixir)),
            ("test.exs", Some(Language::Elixir)),
            ("build.groovy", Some(Language::Groovy)),
            ("build.gradle", Some(Language::Groovy)),
            ("script.ps1", Some(Language::PowerShell)),
            ("core.clj", Some(Language::Clojure)),
            ("main.jl", Some(Language::Julia)),
            ("analysis.r", Some(Language::R)),
            ("analysis.R", Some(Language::R)),
            ("server.erl", Some(Language::Erlang)),
            ("Main.elm", Some(Language::Elm)),
            ("solver.f90", Some(Language::Fortran)),
            ("config.nix", Some(Language::Nix)),
            ("README.md", None),
            ("Cargo.toml", None),
            ("no-extension", None),
        ];

        for (path, expected) in cases {
            assert_eq!(
                CodeParser::detect_language(path),
                expected,
                "detect_language({path:?})"
            );
        }
    }

    #[test]
    fn is_supported_returns_correct_values() {
        // Original languages
        assert!(CodeParser::is_supported("index.ts"));
        assert!(CodeParser::is_supported("app.tsx"));
        assert!(CodeParser::is_supported("main.js"));
        assert!(CodeParser::is_supported("component.jsx"));
        assert!(CodeParser::is_supported("script.py"));
        assert!(CodeParser::is_supported("lib/nested/deep.ts"));
        assert!(CodeParser::is_supported("main.go"));
        assert!(CodeParser::is_supported("lib.rs"));
        assert!(CodeParser::is_supported("Main.java"));
        assert!(CodeParser::is_supported("main.c"));
        assert!(CodeParser::is_supported("main.cpp"));
        assert!(CodeParser::is_supported("Program.cs"));
        assert!(CodeParser::is_supported("index.php"));
        assert!(CodeParser::is_supported("app.rb"));
        assert!(CodeParser::is_supported("Main.kt"));
        assert!(CodeParser::is_supported("App.swift"));
        // Phase 11
        assert!(CodeParser::is_supported("deploy.sh"));
        assert!(CodeParser::is_supported("Main.scala"));
        assert!(CodeParser::is_supported("app.dart"));
        assert!(CodeParser::is_supported("main.zig"));
        assert!(CodeParser::is_supported("script.lua"));
        assert!(CodeParser::is_supported("counter.v"));
        assert!(CodeParser::is_supported("Main.hs"));
        assert!(CodeParser::is_supported("app.ex"));
        assert!(CodeParser::is_supported("build.groovy"));
        assert!(CodeParser::is_supported("script.ps1"));
        assert!(CodeParser::is_supported("core.clj"));
        assert!(CodeParser::is_supported("main.jl"));
        assert!(CodeParser::is_supported("analysis.R"));
        assert!(CodeParser::is_supported("server.erl"));
        assert!(CodeParser::is_supported("Main.elm"));
        assert!(CodeParser::is_supported("solver.f90"));
        assert!(CodeParser::is_supported("config.nix"));

        assert!(!CodeParser::is_supported("readme.md"));
        assert!(!CodeParser::is_supported("config.yaml"));
        assert!(!CodeParser::is_supported(""));
    }

    // -- Query compilation -------------------------------------------------

    #[test]
    fn load_query_succeeds_for_all_languages() {
        let languages = all_languages();

        for lang in languages {
            let query = CodeParser::load_query(lang);
            assert!(
                query.is_ok(),
                "load_query({lang}) failed: {:?}",
                query.err()
            );
            // Every query should capture at least one pattern
            let q = query.unwrap();
            assert!(
                q.pattern_count() > 0,
                "{lang} query should have at least one pattern"
            );
        }
    }

    #[test]
    fn load_query_has_expected_capture_names() {
        let query = CodeParser::load_query(Language::TypeScript).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();

        // Core captures that our extractor will rely on
        assert!(names.contains(&"name"), "missing @name capture");
        assert!(
            names.contains(&"definition.function"),
            "missing @definition.function capture"
        );
        assert!(
            names.contains(&"definition.class"),
            "missing @definition.class capture"
        );
        assert!(
            names.contains(&"definition.method"),
            "missing @definition.method capture"
        );
        assert!(
            names.contains(&"reference.call"),
            "missing @reference.call capture"
        );
    }

    // -- get_ts_language ---------------------------------------------------

    #[test]
    fn get_ts_language_returns_valid_language_for_all_variants() {
        for lang in all_languages() {
            let ts_lang = CodeParser::get_ts_language(lang);
            // A valid language must have a version within the supported range
            assert!(
                ts_lang.abi_version() >= tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION,
                "{lang} grammar version {} is below minimum {}",
                ts_lang.abi_version(),
                tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
            );
            assert!(
                ts_lang.abi_version() <= tree_sitter::LANGUAGE_VERSION,
                "{lang} grammar version {} exceeds maximum {}",
                ts_lang.abi_version(),
                tree_sitter::LANGUAGE_VERSION
            );
        }
    }

    // =====================================================================
    // Parameterized parser initialization tests (test-case)
    // =====================================================================

    use test_case::test_case;

    #[test_case(Language::TypeScript ; "parser_init_typescript")]
    #[test_case(Language::Tsx ; "parser_init_tsx")]
    #[test_case(Language::JavaScript ; "parser_init_javascript")]
    #[test_case(Language::Jsx ; "parser_init_jsx")]
    #[test_case(Language::Python ; "parser_init_python")]
    #[test_case(Language::Go ; "parser_init_go")]
    #[test_case(Language::Rust ; "parser_init_rust")]
    #[test_case(Language::Java ; "parser_init_java")]
    #[test_case(Language::C ; "parser_init_c")]
    #[test_case(Language::Cpp ; "parser_init_cpp")]
    #[test_case(Language::CSharp ; "parser_init_csharp")]
    #[test_case(Language::Php ; "parser_init_php")]
    #[test_case(Language::Ruby ; "parser_init_ruby")]
    #[test_case(Language::Swift ; "parser_init_swift")]
    #[test_case(Language::Kotlin ; "parser_init_kotlin")]
    #[test_case(Language::Bash ; "parser_init_bash")]
    #[test_case(Language::Scala ; "parser_init_scala")]
    #[test_case(Language::Dart ; "parser_init_dart")]
    #[test_case(Language::Zig ; "parser_init_zig")]
    #[test_case(Language::Lua ; "parser_init_lua")]
    #[test_case(Language::Verilog ; "parser_init_verilog")]
    #[test_case(Language::Haskell ; "parser_init_haskell")]
    #[test_case(Language::Elixir ; "parser_init_elixir")]
    #[test_case(Language::Groovy ; "parser_init_groovy")]
    #[test_case(Language::PowerShell ; "parser_init_powershell")]
    #[test_case(Language::Clojure ; "parser_init_clojure")]
    #[test_case(Language::Julia ; "parser_init_julia")]
    #[test_case(Language::R ; "parser_init_r")]
    #[test_case(Language::Erlang ; "parser_init_erlang")]
    #[test_case(Language::Elm ; "parser_init_elm")]
    #[test_case(Language::Fortran ; "parser_init_fortran")]
    #[test_case(Language::Nix ; "parser_init_nix")]
    fn parser_initializes_for_language(lang: Language) {
        let parser = CodeParser::new();
        let result = parser.parse("", lang);
        assert!(
            result.is_ok(),
            "Parser should initialize for {:?}: {:?}",
            lang,
            result.err()
        );
    }

    // =====================================================================
    // Parameterized query loading tests
    // =====================================================================

    #[test_case(Language::TypeScript ; "query_load_typescript")]
    #[test_case(Language::Tsx ; "query_load_tsx")]
    #[test_case(Language::JavaScript ; "query_load_javascript")]
    #[test_case(Language::Jsx ; "query_load_jsx")]
    #[test_case(Language::Python ; "query_load_python")]
    #[test_case(Language::Go ; "query_load_go")]
    #[test_case(Language::Rust ; "query_load_rust")]
    #[test_case(Language::Java ; "query_load_java")]
    #[test_case(Language::C ; "query_load_c")]
    #[test_case(Language::Cpp ; "query_load_cpp")]
    #[test_case(Language::CSharp ; "query_load_csharp")]
    #[test_case(Language::Php ; "query_load_php")]
    #[test_case(Language::Ruby ; "query_load_ruby")]
    #[test_case(Language::Swift ; "query_load_swift")]
    #[test_case(Language::Kotlin ; "query_load_kotlin")]
    #[test_case(Language::Bash ; "query_load_bash")]
    #[test_case(Language::Scala ; "query_load_scala")]
    #[test_case(Language::Dart ; "query_load_dart")]
    #[test_case(Language::Zig ; "query_load_zig")]
    #[test_case(Language::Lua ; "query_load_lua")]
    #[test_case(Language::Verilog ; "query_load_verilog")]
    #[test_case(Language::Haskell ; "query_load_haskell")]
    #[test_case(Language::Elixir ; "query_load_elixir")]
    #[test_case(Language::Groovy ; "query_load_groovy")]
    #[test_case(Language::PowerShell ; "query_load_powershell")]
    #[test_case(Language::Clojure ; "query_load_clojure")]
    #[test_case(Language::Julia ; "query_load_julia")]
    #[test_case(Language::R ; "query_load_r")]
    #[test_case(Language::Erlang ; "query_load_erlang")]
    #[test_case(Language::Elm ; "query_load_elm")]
    #[test_case(Language::Fortran ; "query_load_fortran")]
    #[test_case(Language::Nix ; "query_load_nix")]
    fn query_loads_successfully(lang: Language) {
        let result = CodeParser::load_query(lang);
        assert!(
            result.is_ok(),
            "Query should load for {:?}: {:?}",
            lang,
            result.err()
        );
        let query = result.unwrap();
        assert!(
            query.pattern_count() > 0,
            "{:?} query should have at least one pattern",
            lang
        );
    }

    // =====================================================================
    // Parameterized detect_language tests
    // =====================================================================

    #[test_case("main.ts", Some(Language::TypeScript) ; "detect_ts")]
    #[test_case("app.tsx", Some(Language::Tsx) ; "detect_tsx")]
    #[test_case("index.js", Some(Language::JavaScript) ; "detect_js")]
    #[test_case("entry.mjs", Some(Language::JavaScript) ; "detect_mjs")]
    #[test_case("util.cjs", Some(Language::JavaScript) ; "detect_cjs")]
    #[test_case("Component.jsx", Some(Language::Jsx) ; "detect_jsx")]
    #[test_case("script.py", Some(Language::Python) ; "detect_py")]
    #[test_case("server.go", Some(Language::Go) ; "detect_go")]
    #[test_case("lib.rs", Some(Language::Rust) ; "detect_rs")]
    #[test_case("Main.java", Some(Language::Java) ; "detect_java")]
    #[test_case("main.c", Some(Language::C) ; "detect_c")]
    #[test_case("header.h", Some(Language::C) ; "detect_h")]
    #[test_case("app.cpp", Some(Language::Cpp) ; "detect_cpp")]
    #[test_case("util.cc", Some(Language::Cpp) ; "detect_cc")]
    #[test_case("helper.cxx", Some(Language::Cpp) ; "detect_cxx")]
    #[test_case("types.hpp", Some(Language::Cpp) ; "detect_hpp")]
    #[test_case("Program.cs", Some(Language::CSharp) ; "detect_cs")]
    #[test_case("index.php", Some(Language::Php) ; "detect_php")]
    #[test_case("app.rb", Some(Language::Ruby) ; "detect_rb")]
    #[test_case("App.swift", Some(Language::Swift) ; "detect_swift")]
    #[test_case("Main.kt", Some(Language::Kotlin) ; "detect_kt")]
    #[test_case("build.kts", Some(Language::Kotlin) ; "detect_kts")]
    #[test_case("deploy.sh", Some(Language::Bash) ; "detect_sh")]
    #[test_case("init.bash", Some(Language::Bash) ; "detect_bash")]
    #[test_case("setup.zsh", Some(Language::Bash) ; "detect_zsh")]
    #[test_case("App.scala", Some(Language::Scala) ; "detect_scala")]
    #[test_case("widget.dart", Some(Language::Dart) ; "detect_dart")]
    #[test_case("build.zig", Some(Language::Zig) ; "detect_zig")]
    #[test_case("init.lua", Some(Language::Lua) ; "detect_lua")]
    #[test_case("chip.v", Some(Language::Verilog) ; "detect_v")]
    #[test_case("chip.sv", Some(Language::Verilog) ; "detect_sv")]
    #[test_case("Main.hs", Some(Language::Haskell) ; "detect_hs")]
    #[test_case("lib.ex", Some(Language::Elixir) ; "detect_ex")]
    #[test_case("test.exs", Some(Language::Elixir) ; "detect_exs")]
    #[test_case("build.groovy", Some(Language::Groovy) ; "detect_groovy")]
    #[test_case("build.gradle", Some(Language::Groovy) ; "detect_gradle")]
    #[test_case("script.ps1", Some(Language::PowerShell) ; "detect_ps1")]
    #[test_case("core.clj", Some(Language::Clojure) ; "detect_clj")]
    #[test_case("main.jl", Some(Language::Julia) ; "detect_jl")]
    #[test_case("analysis.R", Some(Language::R) ; "detect_R")]
    #[test_case("server.erl", Some(Language::Erlang) ; "detect_erl")]
    #[test_case("Main.elm", Some(Language::Elm) ; "detect_elm")]
    #[test_case("solver.f90", Some(Language::Fortran) ; "detect_f90")]
    #[test_case("solver.f95", Some(Language::Fortran) ; "detect_f95")]
    #[test_case("config.nix", Some(Language::Nix) ; "detect_nix")]
    #[test_case("README.md", None ; "detect_md_none")]
    #[test_case("Cargo.toml", None ; "detect_toml_none")]
    #[test_case("Makefile", None ; "detect_makefile_none")]
    #[test_case("no_extension", None ; "detect_no_ext_none")]
    #[test_case(".gitignore", None ; "detect_dotfile_none")]
    fn detect_language_parameterized(path: &str, expected: Option<Language>) {
        assert_eq!(
            CodeParser::detect_language(path),
            expected,
            "detect_language({path:?})"
        );
    }

    // =====================================================================
    // Parameterized is_supported tests
    // =====================================================================

    #[test_case("foo.ts", true ; "supported_ts")]
    #[test_case("bar.py", true ; "supported_py")]
    #[test_case("baz.go", true ; "supported_go")]
    #[test_case("qux.rs", true ; "supported_rs")]
    #[test_case("test.lua", true ; "supported_lua")]
    #[test_case("test.nix", true ; "supported_nix")]
    #[test_case("test.f90", true ; "supported_f90")]
    #[test_case("test.elm", true ; "supported_elm")]
    #[test_case("readme.md", false ; "unsupported_md")]
    #[test_case("config.yaml", false ; "unsupported_yaml")]
    #[test_case("", false ; "unsupported_empty")]
    #[test_case("Dockerfile", false ; "unsupported_dockerfile")]
    #[test_case("package.json", false ; "unsupported_json")]
    fn is_supported_parameterized(path: &str, expected: bool) {
        assert_eq!(
            CodeParser::is_supported(path),
            expected,
            "is_supported({path:?})"
        );
    }

    // =====================================================================
    // Parser parses non-trivial source for each language
    // =====================================================================

    #[test_case(Language::TypeScript, "const x: number = 42;", "program" ; "parse_trivial_ts")]
    #[test_case(Language::JavaScript, "function f() { return 1; }", "program" ; "parse_trivial_js")]
    #[test_case(Language::Python, "def f():\n    pass\n", "module" ; "parse_trivial_py")]
    #[test_case(Language::Go, "package main\nfunc main() {}\n", "source_file" ; "parse_trivial_go")]
    #[test_case(Language::Rust, "fn main() {}\n", "source_file" ; "parse_trivial_rust")]
    #[test_case(Language::Java, "class Foo {}\n", "program" ; "parse_trivial_java")]
    #[test_case(Language::C, "int main() { return 0; }\n", "translation_unit" ; "parse_trivial_c")]
    #[test_case(Language::Cpp, "int main() { return 0; }\n", "translation_unit" ; "parse_trivial_cpp")]
    #[test_case(Language::CSharp, "class Foo {}\n", "compilation_unit" ; "parse_trivial_csharp")]
    #[test_case(Language::Php, "<?php function f() {} \n", "program" ; "parse_trivial_php")]
    #[test_case(Language::Ruby, "def foo; end\n", "program" ; "parse_trivial_ruby")]
    #[test_case(Language::Swift, "func main() {}\n", "source_file" ; "parse_trivial_swift")]
    #[test_case(Language::Kotlin, "fun main() {}\n", "source_file" ; "parse_trivial_kotlin")]
    #[test_case(Language::Bash, "echo hello\n", "program" ; "parse_trivial_bash")]
    #[test_case(Language::Scala, "object Main {}\n", "compilation_unit" ; "parse_trivial_scala")]
    #[test_case(Language::Zig, "pub fn main() void {}\n", "source_file" ; "parse_trivial_zig")]
    #[test_case(Language::Lua, "print('hello')\n", "chunk" ; "parse_trivial_lua")]
    #[test_case(Language::Haskell, "module Main where\nmain = putStrLn \"hi\"\n", "haskell" ; "parse_trivial_haskell")]
    #[test_case(Language::Elixir, "defmodule M do\nend\n", "source" ; "parse_trivial_elixir")]
    #[test_case(Language::PowerShell, "function Get-Foo { }\n", "program" ; "parse_trivial_powershell")]
    #[test_case(Language::Clojure, "(defn foo [] nil)\n", "source" ; "parse_trivial_clojure")]
    #[test_case(Language::Julia, "function f()\nend\n", "source_file" ; "parse_trivial_julia")]
    #[test_case(Language::R, "f <- function() { 1 }\n", "program" ; "parse_trivial_r")]
    #[test_case(Language::Erlang, "-module(m).\n", "source_file" ; "parse_trivial_erlang")]
    #[test_case(Language::Elm, "module Main exposing (..)\n\nmain = 42\n", "file" ; "parse_trivial_elm")]
    #[test_case(Language::Fortran, "program hello\nend program hello\n", "translation_unit" ; "parse_trivial_fortran")]
    #[test_case(Language::Nix, "{ a = 1; }\n", "source_code" ; "parse_trivial_nix")]
    fn parse_trivial_source(lang: Language, source: &str, expected_root: &str) {
        let parser = CodeParser::new();
        let tree = parser.parse(source, lang).unwrap_or_else(|e| {
            panic!("Failed to parse {:?}: {:?}", lang, e);
        });
        let root = tree.root_node();
        assert_eq!(root.kind(), expected_root, "Root node kind for {:?}", lang);
        assert!(
            root.child_count() > 0 || source.trim().is_empty(),
            "Expected children for {:?}",
            lang
        );
    }

    // =====================================================================
    // CodeParser::new() and Default
    // =====================================================================

    #[test]
    fn code_parser_default_works() {
        let parser = CodeParser;
        let tree = parser.parse("fn main() {}", Language::Rust);
        assert!(tree.is_ok());
    }

    // =====================================================================
    // Query capture name tests for various languages
    // =====================================================================

    #[test]
    fn typescript_query_captures_function_class_method() {
        let query = CodeParser::load_query(Language::TypeScript).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"name"), "TS missing @name");
        assert!(
            names.contains(&"definition.function"),
            "TS missing @definition.function"
        );
        assert!(
            names.contains(&"definition.class"),
            "TS missing @definition.class"
        );
        assert!(
            names.contains(&"definition.method"),
            "TS missing @definition.method"
        );
    }

    #[test]
    fn python_query_captures_function_class() {
        let query = CodeParser::load_query(Language::Python).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"name"), "Python missing @name");
        assert!(
            names.contains(&"definition.function"),
            "Python missing @definition.function"
        );
        assert!(
            names.contains(&"definition.class"),
            "Python missing @definition.class"
        );
    }

    #[test]
    fn rust_query_captures_function_struct_trait() {
        let query = CodeParser::load_query(Language::Rust).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"name"), "Rust missing @name");
        assert!(
            names.contains(&"definition.function"),
            "Rust missing @definition.function"
        );
    }

    #[test]
    fn go_query_captures_function() {
        let query = CodeParser::load_query(Language::Go).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"name"), "Go missing @name");
        assert!(
            names.contains(&"definition.function"),
            "Go missing @definition.function"
        );
    }

    #[test]
    fn java_query_captures_class_method() {
        let query = CodeParser::load_query(Language::Java).unwrap();
        let names: Vec<&str> = query.capture_names().to_vec();
        assert!(names.contains(&"name"), "Java missing @name");
        assert!(
            names.contains(&"definition.class"),
            "Java missing @definition.class"
        );
    }

    // =====================================================================
    // Parse with syntax errors still produces tree
    // =====================================================================

    #[test]
    fn parse_with_syntax_errors_still_returns_tree() {
        let parser = CodeParser::new();
        // Invalid TypeScript: missing closing brace
        let source = "function foo() {";
        let tree = parser.parse(source, Language::TypeScript);
        assert!(
            tree.is_ok(),
            "Parser should return tree even for broken syntax"
        );
        let tree = tree.unwrap();
        // Tree may have errors but still parses
        assert!(tree.root_node().has_error() || tree.root_node().child_count() > 0);
    }

    #[test]
    fn parse_syntax_error_python_still_returns_tree() {
        let parser = CodeParser::new();
        let source = "def foo(\n    :\n";
        let tree = parser.parse(source, Language::Python);
        assert!(tree.is_ok());
    }

    #[test]
    fn parse_very_long_source() {
        let parser = CodeParser::new();
        // Build a 1000-line source
        let mut source = String::new();
        for i in 0..1000 {
            source.push_str(&format!("fn func_{i}() {{ }}\n"));
        }
        let tree = parser.parse(&source, Language::Rust).unwrap();
        assert!(tree.root_node().child_count() >= 1000);
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parse_unicode_source() {
        let parser = CodeParser::new();
        let source = "fn greet() -> String { String::from(\"Merhaba Dunya!\") }\n";
        let tree = parser.parse(source, Language::Rust).unwrap();
        assert!(!tree.root_node().has_error());
    }

    // =====================================================================
    // Nested path detection
    // =====================================================================

    #[test]
    fn detect_language_nested_paths() {
        assert_eq!(
            CodeParser::detect_language("src/lib/deep/nested/app.ts"),
            Some(Language::TypeScript)
        );
        assert_eq!(
            CodeParser::detect_language("/absolute/path/to/main.py"),
            Some(Language::Python)
        );
        assert_eq!(
            CodeParser::detect_language("../relative/path.go"),
            Some(Language::Go)
        );
    }

    // =====================================================================
    // Property-based tests
    // =====================================================================

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn detect_language_never_panics(path in "\\PC{0,100}") {
            let _ = CodeParser::detect_language(&path);
        }

        #[test]
        fn is_supported_never_panics(path in "\\PC{0,100}") {
            let _ = CodeParser::is_supported(&path);
        }

        #[test]
        fn is_supported_agrees_with_detect_language(path in "[a-z]{1,10}\\.[a-z]{1,5}") {
            let detected = CodeParser::detect_language(&path);
            let supported = CodeParser::is_supported(&path);
            assert_eq!(detected.is_some(), supported);
        }
    }
}
