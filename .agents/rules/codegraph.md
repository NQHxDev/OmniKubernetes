# CodeGraph Usage & Mandatory Code Intelligence Protocol

## 1. Mandatory Policy (Bắt buộc sử dụng)

This project is indexed by **CodeGraph** (`.codegraph/`). All agents working in this repository **MUST** adhere to the following rules:

1. **Prioritize CodeGraph Over Grep / Manual File Reads**:
   - **Do NOT** blindly run `grep`, `rg`, `find`, or repeatedly read entire source files when locating symbols, tracing call paths, or understanding component architecture.
   - **MUST** reach for the `codegraph_explore` MCP tool (or `codegraph explore "<query>"` CLI) first. CodeGraph resolves call hierarchies, type definitions, and dynamic dispatch (e.g., Rust trait implementations) in a single shot with verbatim line-numbered source code.

2. **Pre-modification Blast Radius & Impact Analysis**:
   - Before editing, renaming, or refactoring public functions, structs, traits, or middleware, **MUST** analyze caller impact via `codegraph impact <symbol>` or `codegraph callers <symbol>`.
   - Verify all downstream dependents and affected test suites (`codegraph affected <files...>`) before completing changes.

3. **Index Synchronization**:

    - After adding, deleting, or significantly refactoring source files, run:

      ```bash
      codegraph sync
      ```

      to keep the knowledge graph up to date.

---

## 2. Core Capabilities & Workflows

### A. MCP Tool (Preferred in Agent Environment)

- **`codegraph_explore`**:
  - Input: Symbol names, file paths, or semantic concepts (e.g., `{"query": "middleware context RequestContext"}`).
  - Returns: Verbatim line-numbered source code of matching items, caller/callee paths, and blast radius.
  - **Efficiency Rule**: Output from `codegraph_explore` is fresh from disk. Treat it as already read; **never** execute an additional file read tool for code already presented in the exploration output.

### B. CLI Commands (Quick Reference)

| Task | Command | Description |
| :--- | :--- | :--- |
| **Explore Area** | `codegraph explore "<query>"` | Verbatim source + call paths + blast radius in one shot. |
| **Inspect Symbol / File** | `codegraph node "<symbol or path>"` | Deep inspection with caller/callee trail or file dependents. |
| **Find Callers** | `codegraph callers "<symbol>"` | Discover all functions/methods calling the symbol. |
| **Find Callees** | `codegraph callees "<symbol>"` | Discover all functions/methods called by the symbol. |
| **Analyze Blast Radius** | `codegraph impact "<symbol>"` | Check all downstream dependencies before refactoring. |
| **Affected Tests** | `codegraph affected <file>` | Identify test files affected by changes to source files. |
| **Symbol Search** | `codegraph query "<search>"` | Fast fuzzy symbol lookup across the entire workspace. |
| **Incremental Sync** | `codegraph sync` | Update the index for changes since the last index run. |
| **Status Check** | `codegraph status` | View index statistics and synchronization status. |

---

## 3. Workflow Example for Rust Workspace

```bash
# Step 1: Explore an unfamiliar module or feature
codegraph explore "observability_middleware"

# Step 2: Check who calls a function before changing its signature
codegraph callers "timing_middleware"

# Step 3: Check downstream impact of modifying a struct or trait
codegraph impact "RequestContext"

# Step 4: After editing code, sync index if new symbols were added
codegraph sync
```
