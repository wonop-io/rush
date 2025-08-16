# Rush Output Architecture Analysis

## Current Architecture Overview

### Core Components

1. **OutputSession** (`session.rs`)
   - Central orchestrator for output handling
   - Manages filters, routers, and sinks
   - Created via `SessionBuilder` or from CLI/config

2. **OutputEvent** (`event.rs`)
   - Data structure representing a log event
   - Contains: source, stream, phase, metadata, timestamp

3. **OutputRouter** (`router.rs`)
   - Routes events to appropriate sinks
   - Implementations: BroadcastRouter, RuleBasedRouter, AggregatingRouter

4. **OutputSink** (`sink.rs`)
   - Final destination for events
   - Implementations: TerminalSink, FileSink, BufferSink
   - TerminalSink has different layouts (Linear, Split, Dashboard, etc.)

5. **OutputFormatter** (`formatter.rs`)
   - Formats events for display
   - Implementations: PlainFormatter, ColoredFormatter, JsonFormatter

6. **OutputFilter** (`filter.rs`)
   - Filters events before routing
   - Implementations: ComponentFilter, LevelFilter, PhaseFilter

### Data Flow

```
Docker Container Logs
        ↓
output_integration.rs (follow_container_logs_with_session)
        ↓
Creates OutputEvent
        ↓
OutputSession.submit()
        ↓
Applies Filters (should_pass)
        ↓
Routes via OutputRouter
        ↓
OutputSink.write()
        ↓
Formatter.format()
        ↓
Terminal/File Output
```

## Identified Issues

### Issue 1: CLI Argument Processing
The `--output-format` flag is defined with a default value of "auto" in `args.rs`:
```rust
.arg(arg!(--"output-format" <FORMAT> "...").default_value("auto"))
```

This means `matches.get_one()` will ALWAYS return a value (the default if not specified), so the code should work. But the layout isn't changing.

### Issue 2: Layout Implementation
The TerminalSink has different layout methods, but they're not fully implemented:
- `write_split()` adds `[BUILD]`/`[RUNTIME]` prefixes
- Other layouts are mostly placeholders
- The layout is set correctly in `default_sinks_for_mode()` but may not be persisting

### Issue 3: Color Preservation
Docker containers need TTY allocation (`-t` flag) to produce color output. We added this, but `docker logs` command doesn't preserve TTY colors by default - it just captures stdout/stderr.

### Issue 4: Complex Indirection
The architecture has multiple layers of abstraction:
- OutputDirector (not used in current flow)
- OutputSession → Router → Sink → Formatter
- Multiple builder patterns and factories

## Root Cause Analysis

### Problem 1: Session Creation
Looking at the flow:
1. `execute.rs` calls `create_session_from_cli(dev_matches)`
2. This creates a SessionBuilder and sets the mode
3. SessionBuilder.build() creates sinks based on mode
4. BUT: The sinks are created fresh, losing any configuration

### Problem 2: Docker Logs vs TTY
- `docker run -t` allocates a TTY
- `docker logs` reads from the container's log buffer
- Log buffer doesn't preserve TTY control sequences
- Need to use `docker attach` or capture logs differently

## Proposed Solution

### Simplification Strategy

1. **Reduce Abstraction Layers**
   - Remove unused OutputDirector concept
   - Simplify the Session → Router → Sink chain
   - Make the data flow more direct

2. **Fix Layout Switching**
   - Ensure TerminalSink properly uses the layout
   - Implement proper split-screen rendering (maybe using a TUI library)
   - Or at minimum, ensure prefixes work correctly

3. **Fix Color Preservation**
   - Option A: Use `docker attach` instead of `docker logs`
   - Option B: Configure containers to output with explicit ANSI codes
   - Option C: Add color based on component configuration

4. **Make Configuration Explicit**
   - Log what configuration is actually being used
   - Ensure configuration persists through the chain
   - Add validation to ensure settings are applied

### Immediate Fixes

1. **Debug the Actual Flow**
   - Add logging at each step to see what's actually happening
   - Verify the layout is being set and used

2. **Simplify the Implementation**
   - Focus on making basic split mode work first
   - Remove complex features that aren't working

3. **Fix Docker Color Output**
   - Investigate alternative approaches to preserve colors
   - Consider using Docker SDK directly instead of CLI

## Recommended Refactoring

### Phase 1: Fix Current Implementation
1. Add comprehensive logging to trace the actual flow
2. Ensure layout configuration persists
3. Fix Docker color preservation

### Phase 2: Simplify Architecture
1. Remove unused abstractions (OutputDirector)
2. Consolidate Router implementations
3. Simplify Sink/Formatter relationship

### Phase 3: Enhance Features
1. Implement proper TUI for split/dashboard modes
2. Add better color themes
3. Implement session recording/replay

## Testing Strategy

1. **Unit Tests**
   - Test each component in isolation
   - Verify configuration is preserved

2. **Integration Tests**
   - Test the full pipeline from Docker to terminal
   - Verify different output formats work

3. **Manual Testing**
   - Run with different flags and verify output
   - Test with containers that produce colored output