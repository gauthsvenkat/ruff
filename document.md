# Ruff Symbol Dependency Graph - Component Analysis

## Project Context

This document provides an analysis of the ruff repository to identify components useful for building a **symbol-level dependency graph analyzer** (similar to `ruff graph analyze` but for symbols instead of files).

**Goal**: Implement `ruff analyze symbols` that outputs a dependency graph of symbols in the workspace, reusing existing ruff infrastructure.

**Date**: 2025-11-28

---

## Executive Summary

Ruff already implements most of the infrastructure needed for symbol-level dependency graphs:

- ✅ **File-level dependency graphs** exist (`ruff_graph` crate)
- ✅ **Symbol resolution and indexing** exist (`ty_python_semantic` crate)
- ✅ **Cross-file reference finding** exists (`ty_ide` crate)
- ✅ **Incremental computation** via Salsa database (`ruff_db` crate)

**What's needed**: Wire these together to create symbol→symbol edges instead of file→file edges.

---

## Architecture: Two Semantic Systems

**Critical Understanding**: Ruff has **two independent semantic analysis subsystems** reflecting its dual role as both a linter and a type checker.

### `ruff_python_semantic` - Linter-Focused (Fast, Single-File)

**Location**: `crates/ruff_python_semantic/`

**Purpose**: Lightweight semantic analysis for **linting and code analysis**

- ❌ **No database** - Stateless, disposable
- ❌ **No cross-file** - Analyzes files in isolation
- ❌ **No type inference** - Minimal type information
- ✅ **Very fast** - Arena allocation, single-pass
- ✅ **Stable** - Production-ready

**Core Types**:

- `SemanticModel<'a>` - Lifetime-based (arena-allocated)
- `Binding`, `Scope` - Lightweight binding/scope tracking
- `Module` - Basic module structure

**Used by**: `ruff_linter` (the `ruff check` command)

**Key files**:

- `crates/ruff_python_semantic/src/model.rs:30` - SemanticModel definition
- `crates/ruff_python_semantic/src/binding.rs` - Binding types
- `crates/ruff_python_semantic/src/scope.rs` - Scope implementation

### `ty_python_semantic` - Type Checker-Focused (Complete, Cross-File)

**Location**: `crates/ty_python_semantic/`

**Purpose**: Full-featured semantic analysis for **type checking and IDE support**

- ✅ **Salsa database** - Stateful, cached, incremental
- ✅ **Cross-file** - Workspace-wide symbol resolution
- ✅ **Type inference** - Complete type system (591KB types.rs!)
- ✅ **IDE features** - Powers goto definition, find references, etc.
- 🚀 **Evolving** - Growing type checker subsystem

**Core Types**:

- `SemanticModel<'db>` - Database-backed (Salsa references)
- `Definition`, `Symbol` - Stable cross-file symbol tracking
- `Type`, `TypeDefinition` - Complete type inference
- `Module`, `ModuleName` - Cross-file module resolution
- `SemanticIndex` - Comprehensive symbol indexing

**Used by**: `ty_ide`, `ty_project`, `ty_server` (type checker and LSP)

**Key files**:

- `crates/ty_python_semantic/src/semantic_model.rs:83` - SemanticModel API
- `crates/ty_python_semantic/src/semantic_index/` - Symbol indexing
- `crates/ty_python_semantic/src/types.rs` - Type system
- `crates/ty_python_semantic/src/module_resolver/` - Cross-file resolution

### Key Differences

| Aspect | `ruff_python_semantic` | `ty_python_semantic` |
|--------|------------------------|----------------------|
| **Purpose** | Fast linting | Complete type checking |
| **Scope** | Per-file | Cross-file (workspace) |
| **Database** | None (stateless) | Salsa DB (stateful, cached) |
| **Type Info** | Minimal/Basic | Complete type inference |
| **Memory** | Arena allocation (`'a`) | Salsa references (`'db`) |
| **Performance** | Very fast | Slower (complete analysis) |
| **Maturity** | Stable | Growing |
| **Consumers** | `ruff_linter` | `ty_ide`, `ty_project` |
| **Cross-file** | ❌ No | ✅ Yes |

### No Cross-Dependency

**Important**: Neither depends on the other! They are independent implementations:

```
        ruff_python_ast (shared AST)
              ↑
    ┌─────────┼─────────┐
    ↓                   ↓
ruff_python_semantic   ty_python_semantic
    ↓                   ↓
ruff_linter        ty_ide, ty_project
```

### When to Use Which

**For symbol dependency graphs**: Use `ty_python_semantic` because you need:

- ✅ Cross-file symbol resolution
- ✅ Workspace-wide reference finding
- ✅ Stable symbol IDs that persist across queries
- ✅ Salsa caching for incremental updates

**Only use** `ruff_python_semantic` if building a single-file linting rule.

### Other `ruff_*` vs `ty_*` Pairs

The pattern is systemic across the codebase:

| Component | ruff_* | ty_* | Difference |
|-----------|--------|------|------------|
| Semantic | `ruff_python_semantic` | `ty_python_semantic` | Single-file vs cross-file |
| Server | `ruff_server` | `ty_server` | Linter LSP vs type checker LSP |
| Database | `ruff_db` | `ty_project` | Basic files vs project context |

The `ty_` prefix indicates **type-aware infrastructure** built on Salsa database.

---

## 1. Module-Level Dependency Tracking (`ruff_graph`)

**Location**: `crates/ruff_graph/`

This crate provides the foundation for the existing file-level graph analysis.

### Key Components

#### `ModuleImports`

Tracks file-to-file import dependencies.

```rust
// Detects imports from Python source code
// Resolves both `import` and `from ... import` statements
// Handles string imports and TYPE_CHECKING blocks
```

**Methods**:

- `detect()` - Extract imports from source
- `insert()` - Add import relationship
- `extend()` - Bulk add imports
- `relative_to()` - Convert to relative imports

#### `ImportMap`

Maps files to their dependencies/dependents.

```rust
// Creates bidirectional dependency maps
ImportMap::dependencies() // file -> files it imports
ImportMap::dependents()   // file -> files that import it
```

#### `Collector`

AST visitor that extracts imports.

**Features**:

- Traverses Python AST in source order
- Handles relative imports with proper scope resolution
- Filters imports based on configuration

**Location**: `crates/ruff_graph/src/collector.rs`

#### `Resolver`

Resolves import names to actual file paths.

**Features**:

- Uses `resolve_module()` from `ty_python_semantic`
- Handles stub files (.pyi) and source files
- Works with both module members and parent modules

**Location**: `crates/ruff_graph/src/resolver.rs`

#### `ModuleDb`

Database for module resolution.

**Features**:

- Uses Salsa-based incremental computation
- Integrates with `ty_python_semantic` for module resolution
- Handles Python environments and search paths

### Key Files

- `crates/ruff_graph/src/lib.rs`
- `crates/ruff_graph/src/collector.rs`
- `crates/ruff_graph/src/resolver.rs`

### Reusability

**HIGH** - Can form the foundation of file-level graph layer. The import extraction and resolution logic is production-ready.

---

## 2. Symbol Resolution and Indexing (`ty_python_semantic`)

**Location**: `crates/ty_python_semantic/src/semantic_index/`

This is the type checker's core semantic analysis infrastructure.

### Symbol Table & Place Tracking

#### `Symbol`

Represents a symbol in a scope with metadata.

**Flags**:

- `IS_USED` - Symbol is referenced
- `IS_BOUND` - Symbol has a value
- `IS_DECLARED` - Symbol is explicitly declared
- `MARKED_GLOBAL` - Has `global` keyword
- `MARKED_NONLOCAL` - Has `nonlocal` keyword
- `IS_REASSIGNED` - Assigned multiple times
- `IS_PARAMETER` - Function parameter

**Methods**:

- `is_used()`, `is_bound()`, `is_declared()`
- `is_local()` - Check if symbol is local to scope

**Location**: `crates/ty_python_semantic/src/semantic_index/symbol.rs`

#### `SymbolTable`

Hash-table-based symbol lookup within a scope.

**Features**:

- Maps symbol names to `ScopedSymbolId`
- Provides O(1) lookup by name
- Tracks all symbols in a scope

#### `ScopedSymbolId` / `ScopedPlaceId`

Unique identifiers for symbols and places within a scope.

### Definition Tracking

#### `Definition`

Salsa-tracked struct representing a symbol definition.

**Tracks**:

- File
- Scope
- Place
- Kind (function/class/type alias/etc)

**Methods**:

- `name()` - Symbol name
- `docstring()` - Documentation
- `full_range()` - Complete range including body
- `focus_range()` - Just the name/identifier
- Supports cross-file queries with stable IDs

**Location**: `crates/ty_python_semantic/src/semantic_index/definition.rs`

#### `DefinitionKind`

Enumeration of different definition types:

- Function
- Class
- TypeAlias
- Assignment
- NamedExpression
- Parameter
- etc.

### Scope Architecture

#### `ScopeId<'db>`

Cross-module scope identifier (file + FileScopeId).

#### `FileScopeId`

Per-file unique scope identifier.

#### `Scope`

Represents a lexical scope.

**Contains**:

- Parent scope reference
- AST node introducing scope
- Range of descendant scopes
- Reachability constraints
- TYPE_CHECKING block marker

**Location**: `crates/ty_python_semantic/src/semantic_index/scope.rs`

### Use-Def Chains

**Location**: `crates/ty_python_semantic/src/semantic_index/use_def.rs`

Tracks control-flow-sensitive definitions and uses.

### Key Files

- `crates/ty_python_semantic/src/semantic_index/symbol.rs`
- `crates/ty_python_semantic/src/semantic_index/definition.rs`
- `crates/ty_python_semantic/src/semantic_index/scope.rs`
- `crates/ty_python_semantic/src/semantic_index/use_def.rs`

### Reusability

**HIGH** - This is the core symbol tracking infrastructure. Provides stable symbol IDs and metadata needed for graph nodes.

---

## 3. Semantic Model (`ty_python_semantic`)

**Location**: `crates/ty_python_semantic/src/semantic_model.rs:83`

Primary interface for querying semantic information.

### `SemanticModel`

Main API for a file's semantic analysis.

**Key Methods**:

- `members_in_scope_at(node)` - Get all symbols visible at a location
- `resolve_module(module, level)` - Resolve imports to modules
- `resolve_module_type()` - Get type of imported module
- `import_completions()` - List all available modules
- `from_import_completions()` - List members of a module

**Features**:

- Handles string annotation parsing
- Provides scope-aware symbol lookup
- Integrates control-flow-sensitive type information

### Reusability

**HIGH** - Essential API for querying "what symbols exist at this point in the code?"

---

## 4. Module Resolution (`ty_python_semantic`)

**Location**: `crates/ty_python_semantic/src/module_resolver/`

Cross-file module and symbol resolution.

### `Module`

Represents a Python module.

**Methods**:

- `name()` - Module name
- `file()` - File path
- `is_known()` - Is it a known module?
- `all_submodules()` - List all submodules

**Types**:

- Builtins (e.g., `sys`, `os`)
- Typeshed stubs
- Site-packages
- User modules

### Module Resolution Functions

#### `resolve_module(db, module_name)`

Global function to resolve module names.

**Features**:

- Searches through all search paths
- Handles package hierarchies
- Returns Module structs

#### `resolve_real_module()`

Variant that skips stub files (.pyi).

#### `all_modules(db)`

Returns iterator over all modules in workspace.

#### `list_modules(db)`

Lists available modules for completion.

### `SearchPath`

Represents a location to search for modules.

**Types**:

- Filesystem
- Vendored typeshed
- Site-packages

### Key Files

- `crates/ty_python_semantic/src/module_resolver/mod.rs`
- `crates/ty_python_semantic/src/module_resolver/resolver.rs`

### Reusability

**HIGH** - Essential for resolving cross-file symbol references and understanding the module structure.

---

## 5. Cross-File Reference and Dependency Tracking (`ty_ide`)

**Location**: `crates/ty_ide/src/`

IDE features that track symbol usage across files. This is **critical** for building symbol dependency graphs.

### References & Find-All-References

#### `goto_references(db, file, offset, include_declaration)`

Finds all references to a symbol across workspace.

**Location**: `crates/ty_ide/src/goto_references.rs:24`

**Returns**: `ReferenceTarget` with kind (Read/Write/Other)

**Process**:

1. Gets definition targets for symbol at cursor
2. Searches current file for references
3. If symbol is externally visible, searches all files
4. Uses fast text search first, then semantic validation

**Implementation**: `crates/ty_ide/src/references.rs:42`

#### `ReferenceTarget`

Includes file range and reference kind.

#### `ReferenceKind`

- `Read` - Reading a value
- `Write` - Assigning to a symbol
- `Other` - Other uses

### Symbol Extraction

#### `symbols_for_file_global_only(db, file)`

Extracts all top-level symbols from a file.

#### `workspace_symbols(db, query)`

Searches symbols across all files.

**Location**: `crates/ty_ide/src/workspace_symbols.rs:30`

**Features**:

- Parallel search using rayon
- Fuzzy matching for queries
- Returns `WorkspaceSymbolInfo`

#### `document_symbols(db, file)`

Gets symbols in a file (hierarchical).

**Returns**: `HierarchicalSymbols` - Tree structure with parent/child relationships

#### `all_symbols(db, query)`

Searches all modules including dependencies.

### Symbol Metadata

#### `SymbolInfo`

Contains:

- Name
- Kind (Class/Function/Variable/Module)
- Range in source
- Optional docstring
- Parent scope

#### `SymbolId`

Unique ID within a file.

#### `WorkspaceSymbolInfo`

Symbol + file location.

### Key Files

- `crates/ty_ide/src/references.rs:42` - Core reference finding
- `crates/ty_ide/src/goto_references.rs:24` - LSP goto references
- `crates/ty_ide/src/workspace_symbols.rs:30` - Workspace-wide symbol search
- `crates/ty_ide/src/all_symbols.rs` - All symbols including dependencies
- `crates/ty_ide/src/symbols.rs` - Symbol extraction

### Reusability

**VERY HIGH** - This is the key component. `goto_references()` is exactly what you need to find symbol→symbol dependencies.

---

## 6. Definition and Type Tracking (`ty_ide`)

**Location**: `crates/ty_ide/src/goto*.rs`

Navigation features for IDE.

### Navigation Functions

#### `goto_definition(db, file, offset)`

Navigate to symbol definition.

**Location**: `crates/ty_ide/src/goto_definition.rs:41`

#### `goto_declaration()`

Navigate to declaration (type annotation).

#### `goto_type_definition()`

Navigate to type definition.

### Navigation Types

#### `NavigationTarget`

Represents a location to navigate to.

**Fields**:

- File
- focus_range - The identifier
- full_range - Complete definition including body

**Can be converted to LSP Location**

#### `GotoTarget`

Represents what's being navigated from.

**Features**:

- Wraps various AST node types
- Provides navigation-specific methods

### Reusability

**MEDIUM** - Useful for understanding definition locations, but `goto_references()` is more important for dependency graphs.

---

## 7. LSP Server Integration (`ty_server`)

**Location**: `crates/ty_server/src/server/api/requests/`

Handler implementations for IDE features. Shows how to use the `ty_ide` APIs.

### Request Handlers

#### `ReferencesRequestHandler`

Implements LSP `textDocument/references`.

**Pattern**:

1. Get document snapshot
2. Convert position to text offset
3. Call `ty_ide::goto_references()`
4. Convert results to LSP `Location`

#### `DocumentSymbolsHandler`

Implements LSP `textDocument/documentSymbol`.

#### `WorkspaceSymbolsHandler`

Implements LSP `workspace/symbol`.

#### `GotoDefinitionHandler`

Implements LSP `textDocument/definition`.

### Reusability

**MEDIUM** - Good reference for how to use the `ty_ide` APIs, but you won't need the LSP protocol conversion.

---

## 8. Salsa Database Infrastructure (`ruff_db`)

**Location**: `crates/ruff_db/`

Incremental computation engine that powers all of ruff's semantic analysis.

### `Db` trait

Main database interface.

**Methods**:

- `files()` - Access file system
- `system()` - Access OS operations
- `vendored()` - Access typeshed stubs

### `File`

Unique file identifier.

**Queries**:

- Path
- Content
- Parsed AST

### `Files`

File registry.

**Features**:

- Tracks all files in workspace
- Provides file indexing

### Features

- **Incremental re-computation** - Only recompute what changed
- **Caching** - Expensive operations cached
- **Parallel processing** - Uses Rayon for multi-threading

### Reusability

**HIGH** - You'll use the existing Db infrastructure. Don't need to modify it, but need to understand it.

---

## 9. Existing Graph Analysis Command

**Location**: `crates/ruff/src/commands/analyze_graph.rs:48`

Current `ruff analyze graph` implementation.

### How It Works

```rust
// 1. Parse all Python files in workspace
// 2. Use ruff_graph to extract imports
// 3. Generate ImportMap (module dependencies)
// 4. Output JSON
```

### Configuration Options

- `string_imports` - Include imports from string literals
- `type_checking_imports` - Include TYPE_CHECKING blocks
- `include_dependencies` - Additional static dependencies via globs

### Output Format

JSON with:

- Direction: `dependencies` or `dependents`
- Map: file → list of related files

### Limitations (Current)

- ❌ Only file-level imports, not symbol-level
- ❌ No cross-file symbol reference tracking
- ❌ No type-aware dependency analysis

### Reusability

**MEDIUM** - Good template for CLI structure and output format. Symbol version would follow similar pattern but use different data structures.

---

## Architecture Analysis

### Reusability Matrix

| Component | Reusability | What You Can Use |
|-----------|-------------|------------------|
| `ruff_graph::ImportMap` | **HIGH** | File-level dependency foundation |
| `ty_ide::goto_references()` | **VERY HIGH** | Cross-file symbol reference finding |
| `ty_python_semantic::SemanticModel` | **HIGH** | Symbol resolution API |
| `ty_python_semantic::Definition` | **HIGH** | Symbol definition tracking with stable IDs |
| `ty_python_semantic::Module` | **HIGH** | Module abstraction and resolution |
| `ty_ide::workspace_symbols()` | **HIGH** | Index all symbols in workspace |
| `ruff_graph::Collector` | **HIGH** | Import extraction (well-tested) |
| `ty_ide::SymbolInfo` | **MEDIUM** | Need cross-file aggregation |
| `ruff_db::Db` | **HIGH** | Use existing infrastructure |
| LSP handlers | **MEDIUM** | Reference implementations only |

### Key Architectural Patterns

1. **Salsa Database**: All major queries are cached and incrementally recomputed
2. **Semantic Model**: Primary API for per-file queries
3. **AST Visitors**: Source-order traversal for import collection
4. **Parallel Processing**: rayon for multi-file analysis
5. **LSP Integration**: Already supports IDE features needed for navigation

---

## Recommended Implementation Approach

### Layered Architecture

```
Symbol Dependency Graph
├── File-level layer (EXISTS: ruff_graph::ImportMap)
├── Module-level layer (EXISTS: ty_python_semantic::Module)
└── Symbol-level layer (NEW: build this)
    ├── Symbol Index (collect all symbols from all files)
    ├── Reference Graph (goto_references for each symbol)
    ├── Dependency Classification (import/reference/type-use)
    └── Visualization/Query API
```

### Step-by-Step Implementation

#### Step 1: Symbol Index

Build a workspace-wide symbol index.

```rust
// Use ty_ide::workspace_symbols() to get all symbols
// Build map: SymbolId -> SymbolInfo + Definition
// Handle deduplication across files
```

**Reuse**:

- `ty_ide::workspace_symbols()`
- `ty_python_semantic::Definition`
- `ty_ide::SymbolInfo`

#### Step 2: Reference Graph

For each symbol, find all references.

```rust
// For each symbol in index:
//   Use ty_ide::goto_references() to find all uses
//   Link each use back to the definition
//   Store as edges in graph
```

**Reuse**:

- `ty_ide::goto_references()`
- `ty_ide::ReferenceTarget`
- `ty_ide::ReferenceKind`

#### Step 3: Dependency Classification

Classify the type of dependency.

```rust
// For each reference edge:
//   Classify as: Import, Call, Inheritance, Type Annotation, etc.
//   Use AST node type and context
```

**Reuse**:

- `ruff_python_ast` - AST node types
- `ReferenceKind` - Read/Write/Other as starting point

#### Step 4: File-Level Import Integration

Merge with existing file-level graph.

```rust
// Use ruff_graph::ImportMap for file imports
// Add as file-level nodes in the graph
// Or use to validate symbol-level dependencies
```

**Reuse**:

- `ruff_graph::ImportMap`
- `ruff_graph::Collector`

#### Step 5: Query API and Output

Create API for querying the graph.

```rust
// Functions:
//   get_dependencies(symbol) -> Vec<SymbolDependency>
//   get_dependents(symbol) -> Vec<SymbolDependent>
//   dependency_path(from, to) -> Option<Path>
//
// Output formats:
//   JSON (like current ruff analyze graph)
//   DOT format for graphviz
//   Mermaid for documentation
```

### Pseudocode

```rust
pub struct SymbolDependencyGraph {
    // Symbol ID -> Symbol metadata
    symbols: HashMap<SymbolId, SymbolNode>,
    // Symbol ID -> List of symbols it depends on
    dependencies: HashMap<SymbolId, Vec<SymbolEdge>>,
    // Symbol ID -> List of symbols that depend on it
    dependents: HashMap<SymbolId, Vec<SymbolEdge>>,
}

pub struct SymbolNode {
    id: SymbolId,
    name: String,
    kind: SymbolKind,
    file: File,
    definition: Definition,
}

pub struct SymbolEdge {
    from: SymbolId,
    to: SymbolId,
    kind: DependencyKind,
    references: Vec<ReferenceTarget>,
}

pub enum DependencyKind {
    Import,           // from foo import bar
    Call,             // bar()
    Inheritance,      // class Foo(bar):
    TypeAnnotation,   // x: bar
    Assignment,       // x = bar
    AttributeAccess,  // bar.baz
}

// Build the graph
pub fn build_symbol_graph(db: &dyn Db, workspace: &Workspace) -> SymbolDependencyGraph {
    let mut graph = SymbolDependencyGraph::new();

    // Step 1: Index all symbols
    let all_symbols = workspace_symbols(db, ""); // Empty query = all symbols
    for symbol_info in all_symbols {
        let definition = get_definition_for_symbol(db, &symbol_info);
        graph.add_symbol(symbol_info, definition);
    }

    // Step 2: Find references for each symbol
    for (symbol_id, symbol_node) in &graph.symbols {
        let references = goto_references(
            db,
            symbol_node.file,
            symbol_node.definition.focus_range().start(),
            false, // exclude_declaration
        );

        for reference in references {
            let kind = classify_dependency(db, &reference);
            graph.add_edge(symbol_id, reference, kind);
        }
    }

    graph
}
```

---

## Key Entry Points to Study

### For Understanding Current Implementation

1. **`crates/ruff/src/commands/analyze_graph.rs:48`**
   - Current file-level graph command
   - Shows CLI structure and output format

2. **`crates/ruff_graph/src/lib.rs`**
   - Import map data structures
   - How file dependencies are represented

### For Symbol Resolution

3. **`crates/ty_python_semantic/src/semantic_index/definition.rs`**
   - How symbols are defined and tracked
   - Stable symbol IDs

4. **`crates/ty_python_semantic/src/semantic_model.rs:83`**
   - Main API for semantic queries
   - How to ask "what symbols are visible here?"

### For Cross-File Analysis

5. **`crates/ty_ide/src/references.rs:42`**
   - **MOST IMPORTANT**: Core implementation of reference finding
   - Shows how to find all uses of a symbol across workspace

6. **`crates/ty_ide/src/workspace_symbols.rs:30`**
   - How to index all symbols in workspace
   - Parallel processing pattern

### For Integration

7. **`crates/ty_server/src/server/api/requests/references.rs`**
   - Example of using `goto_references()` from LSP
   - Shows complete flow from request to response

---

## Data Flow Diagram

```
User Request: "ruff analyze symbols"
│
├─> Parse all files in workspace (ruff_db::Files)
│
├─> Build Symbol Index
│   ├─> ty_ide::workspace_symbols() → All symbols
│   ├─> ty_python_semantic::Definition → Symbol metadata
│   └─> Store in SymbolDependencyGraph.symbols
│
├─> Build Reference Graph
│   ├─> For each symbol:
│   │   ├─> ty_ide::goto_references() → All uses
│   │   ├─> Classify dependency type (Import/Call/Type/etc)
│   │   └─> Add edges to graph
│   └─> Build bidirectional maps (dependencies + dependents)
│
├─> Optional: Add File-Level Imports
│   ├─> ruff_graph::ImportMap → File dependencies
│   └─> Merge with symbol graph
│
└─> Output
    ├─> JSON format (like current command)
    ├─> DOT format (graphviz)
    └─> Or custom format
```

---

## Current Gaps and Building Blocks Needed

### What Exists ✅

1. Symbol definition tracking (`Definition`)
2. Cross-file reference finding (`goto_references`)
3. Symbol indexing (`workspace_symbols`)
4. Module resolution (`resolve_module`)
5. File-level import graph (`ImportMap`)
6. Incremental computation (Salsa/`ruff_db`)

### What You Need to Build 🔨

1. **Symbol-to-Symbol Dependency Graph**
   - Currently: file imports → file
   - Needed: symbol uses → symbol definition
   - **Solution**: Use `goto_references()` for each symbol

2. **Cross-File Symbol Index**
   - Currently: Per-file symbol tables
   - Needed: Workspace-wide unified index
   - **Solution**: Aggregate `workspace_symbols()` + `all_symbols()`

3. **Dependency Type Classification**
   - Currently: Generic references
   - Needed: Import vs. call vs. type vs. inheritance
   - **Solution**: Examine AST context of each reference

4. **Scope-Aware Visibility Rules**
   - Currently: Basic public/private
   - Needed: Re-exports, aliasing, `__all__`
   - **Solution**: Use `Scope::visibility()` and definition metadata

5. **Graph Query API**
   - Currently: N/A
   - Needed: Query dependencies, dependents, paths
   - **Solution**: Standard graph traversal algorithms

6. **Output Format**
   - Currently: JSON for file graph
   - Needed: Symbol graph JSON/DOT/Mermaid
   - **Solution**: Serialize graph structure

---

## Performance Considerations

### Incremental Computation

- Salsa automatically caches symbol resolution
- Only recompute when files change
- `goto_references()` uses text search + semantic validation

### Parallel Processing

- `workspace_symbols()` uses rayon for parallel file processing
- Can parallelize reference finding across symbols
- File parsing is already parallel

### Memory Usage

- Large workspaces may have 100k+ symbols
- Consider streaming output for large graphs
- Use symbol IDs instead of full names in edges

### Optimization Strategies

1. **Cache reference results**: Store in Salsa query
2. **Filter by visibility**: Skip private symbols for public API graphs
3. **Incremental updates**: Only reprocess changed files
4. **Lazy loading**: Build graph on-demand per file/module

---

## Testing Strategy

### Unit Tests

- Test symbol indexing on small files
- Test reference finding for different symbol types
- Test dependency classification logic

### Integration Tests

- Test on real Python projects (e.g., small stdlib modules)
- Verify correctness against manual inspection
- Compare with existing tools (pydeps, etc.)

### Performance Tests

- Benchmark on large codebases (100+ files)
- Profile memory usage
- Test incremental update speed

---

## Example Usage (Proposed)

```bash
# Analyze all symbols in workspace
ruff analyze symbols

# Output to file
ruff analyze symbols --output symbols.json

# Filter to specific module
ruff analyze symbols --module myproject.core

# Show only public API
ruff analyze symbols --visibility public

# Output as DOT for graphviz
ruff analyze symbols --format dot

# Show dependencies of specific symbol
ruff analyze symbols --symbol MyClass.my_method

# Show dependents (reverse)
ruff analyze symbols --symbol MyClass --direction dependents
```

---

## Next Steps

1. **Familiarize** with key entry points (listed above)
2. **Prototype** symbol index builder using `workspace_symbols()`
3. **Test** reference finding with `goto_references()` on small examples
4. **Design** graph data structure (nodes, edges, metadata)
5. **Implement** dependency classification logic
6. **Build** CLI command following `analyze_graph.rs` pattern
7. **Add** output formats (JSON, DOT, etc.)
8. **Optimize** for large workspaces

---

## References

### Ruff Documentation

- Main repo: <https://github.com/astral-sh/ruff>
- Ruff graph docs: (check docs/ folder)

### Salsa

- Salsa book: <https://salsa-rs.github.io/salsa/>

### Related Tools

- pydeps: Python module dependency graphs
- pyreverse: UML diagrams from Python code
- sourcetrail: Interactive source explorer

---

## Questions & Considerations

### Open Questions

1. Should we include stdlib symbols in the graph?
2. How to handle dynamic imports (`importlib`)?
3. Should we track attribute access (`obj.attr`) as dependencies?
4. How to represent re-exports and aliasing?
5. Should we differentiate read vs. write references in the graph?

### Design Decisions

1. **Granularity**: Symbol-level vs. function-level vs. statement-level?
2. **Visibility**: Include private symbols or just public API?
3. **External dependencies**: Include site-packages or just workspace?
4. **Edge types**: How many dependency kinds to distinguish?

---

## Changelog

- **2025-11-28**: Initial analysis document created
- **2025-11-28**: Added architectural analysis of `ruff_python_semantic` vs `ty_python_semantic` - clarified the dual semantic systems (linter vs type checker)
