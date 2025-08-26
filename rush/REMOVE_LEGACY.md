# Legacy Code Removal Plan for ContainerReactor

## Overview
The ContainerReactor in `rush-container/src/reactor/core.rs` contains significant legacy code from the Phase 6 modular refactoring. This document outlines a plan to remove all legacy code and fully transition to the modular architecture.

## Current State Analysis

### Legacy Code Identified

1. **Legacy Fields in ContainerReactor struct** (lines 89-91)
   - `legacy_component_specs: Vec<ComponentBuildSpec>`
   - `legacy_available_components: Vec<String>`
   - These are maintained "for API compatibility" but no longer needed

2. **Legacy Methods**
   - `build_all_legacy()` (lines 903-1163) - Full legacy build implementation
   - `run_legacy()` (lines 2712-2719) - Stub legacy run implementation
   - These are fallback implementations when modular reactor fails

3. **Legacy Configuration**
   - Conservative defaults for backward compatibility:
     - `auto_restart: false` (line 179, 2687)
     - `enable_health_checks: false` (line 180, 2688)
     - `use_enhanced_client: false` (line 2702)

4. **TODO/Incomplete Implementations**
   - Docker push (line 605)
   - Kubectl context selection (line 617)
   - K8s manifest generation and operations (lines 631, 641, 651, 661, 671)
   - Component ID generation using "TODO" placeholder (line 497)

5. **Legacy Compatibility Logic**
   - Fallback to legacy when modular reactor unavailable (lines 2658-2660, 2674)
   - Legacy field population in `from_product_dir()` (lines 419-421)

## Removal Plan

### Phase 1: Remove Legacy Fields and Their Usage
**Files to modify:**
- `rush-container/src/reactor/core.rs`

**Tasks:**
1. Remove `legacy_component_specs` and `legacy_available_components` fields from ContainerReactor struct
2. Remove all references to these fields:
   - Lines 249-250: Initialization in `new()`
   - Lines 419-421: Population in `from_product_dir()`
   - Lines 2765-2770: Accessor methods `component_specs()` and `component_specs_mut()`
3. Update `component_specs()` and `component_specs_mut()` to use the modular reactor's state directly

### Phase 2: Remove Legacy Methods
**Files to modify:**
- `rush-container/src/reactor/core.rs`

**Tasks:**
1. Delete `build_all_legacy()` method entirely (lines 902-1163)
2. Delete `run_legacy()` method entirely (lines 2711-2719)
3. Update `build_all()` to directly fail if modular reactor is not available (no fallback)
4. Update `launch_loop()` to require modular reactor (no fallback)

### Phase 3: Update Configuration Defaults
**Files to modify:**
- `rush-container/src/reactor/core.rs`
- `rush-container/src/reactor/factory.rs`

**Tasks:**
1. Enable modern features by default:
   - Set `auto_restart: true`
   - Set `enable_health_checks: true`
   - Set `use_enhanced_client: true`
2. Remove "legacy compatibility" comments
3. Update factory to use production-ready defaults

### Phase 4: Implement Missing Functionality
**Files to modify:**
- `rush-container/src/reactor/core.rs`
- Create new files as needed for K8s operations

**Tasks:**
1. Implement proper component ID generation (replace "TODO" placeholder)
2. Implement Docker push functionality
3. Implement K8s operations:
   - Create `k8s_operations.rs` module
   - Move K8s methods to dedicated module
   - Implement actual functionality or clearly mark as out-of-scope

### Phase 5: Simplify Architecture
**Files to modify:**
- `rush-container/src/reactor/core.rs`
- `rush-container/src/reactor/modular_core.rs`

**Tasks:**
1. Make `ModularReactor` the primary implementation
2. Consider renaming `ModularReactor` to `Reactor`
3. Move essential bridging logic from `ContainerReactor` directly into the new `Reactor`
4. Remove the optional modular_reactor field - make it mandatory

### Phase 6: Clean Up Comments and Documentation
**Files to modify:**
- All reactor-related files

**Tasks:**
1. Remove all "legacy", "compatibility", "preserved for reference" comments
2. Update struct documentation to reflect current architecture
3. Remove migration-related types and enums if no longer needed
4. Update README/documentation to reflect the new architecture

## Implementation Order

1. **Start with Phase 1** - Remove unused legacy fields (lowest risk)
2. **Then Phase 2** - Remove legacy methods (medium risk, requires testing)
3. **Then Phase 3** - Update defaults (requires validation that features work)
4. **Then Phase 5** - Simplify architecture (highest impact, do before adding new features)
5. **Then Phase 4** - Implement missing features (can be done incrementally)
6. **Finally Phase 6** - Clean up documentation

## Testing Strategy

Before each phase:
1. Run existing test suite
2. Test basic dev workflow: `rush helloworld.wonop.io dev`
3. Test build workflow: `rush helloworld.wonop.io build`
4. Test with force-rebuild flag
5. Verify file watching still works
6. Verify domain generation is correct

## Migration Risks

1. **Breaking API changes** - External code may depend on legacy methods
   - Mitigation: Search codebase for all usages before removal
   
2. **Hidden dependencies** - Legacy code may have subtle interactions
   - Mitigation: Incremental removal with testing between phases
   
3. **Performance regression** - Modular code may have different performance characteristics
   - Mitigation: Profile before and after changes

### Phase 7: Complete Reactor Replacement (Optional)
**Files to modify:**
- `rush-container/src/reactor/core.rs` - Remove entirely
- `rush-container/src/reactor/mod.rs` - Update exports
- `rush-container/src/lib.rs` - Update public API
- `rush-cli/src/context_builder.rs` - Update to use Reactor directly
- All files that import `ContainerReactor`

**Context:**
Currently, `ContainerReactor` acts as a wrapper that delegates to the primary `Reactor`. This phase would complete the replacement by removing `ContainerReactor` entirely and having all code use `Reactor` directly.

**Tasks:**
1. **Update all external usage:**
   - Find all imports of `ContainerReactor` in rush-cli and other crates
   - Replace `ContainerReactor::from_product_dir()` with equivalent `Reactor` construction
   - Update `CliContext` to use `Reactor` instead of `ContainerReactor`

2. **Migrate initialization logic:**
   - Move the `from_product_dir()` factory method to `Reactor` or a dedicated builder
   - Ensure all initialization logic (network setup, component loading, etc.) is preserved
   - Consider creating a `ReactorBuilder` for complex initialization

3. **Update public API:**
   - Change `rush_container::ContainerReactor` export to `rush_container::Reactor`
   - Consider keeping a type alias for backward compatibility: `pub type ContainerReactor = Reactor;`

4. **Remove ContainerReactor:**
   - Delete `rush-container/src/reactor/core.rs`
   - Remove ContainerReactor export from `mod.rs`
   - Update factory to no longer reference ContainerReactor

**Considerations:**
- **Breaking Change**: This would be a breaking change for any external code using `ContainerReactor`
- **Migration Path**: Could provide a type alias temporarily: `pub type ContainerReactor = Reactor;`
- **Initialization Complexity**: The `from_product_dir()` method in ContainerReactor handles complex initialization that needs to be preserved
- **Testing Impact**: All tests using ContainerReactor would need updates

**Alternative Approach:**
Instead of removing ContainerReactor, it could be kept as a thin facade that:
1. Provides backward compatibility
2. Handles product-specific initialization
3. Eventually becomes just a type alias

**Decision Point:**
This phase is optional because:
- The current delegation pattern works and maintains compatibility
- ContainerReactor provides a stable API while the underlying Reactor can evolve
- The cost/benefit depends on how much external code depends on ContainerReactor

## Success Criteria

- [x] No legacy fields in ContainerReactor struct (Phase 1-2 completed)
- [x] No legacy methods (build_all_legacy, run_legacy) (Phase 2 completed)
- [x] No fallback logic to legacy implementations (Phase 2-3 completed)
- [x] Modern features enabled by default (Phase 3 completed)
- [x] Clear error messages when operations are unsupported (Phase 4 completed)
- [x] All tests passing (All phases)
- [x] Dev and build commands working correctly (Verified)
- [x] Cleaner, more maintainable codebase (Phase 5-6 completed)
- [x] Complete replacement of ContainerReactor with Reactor (Phase 7 - completed)

## Estimated Timeline

- Phase 1: 30 minutes ✓
- Phase 2: 1 hour (including testing) ✓
- Phase 3: 30 minutes ✓
- Phase 4: 2-4 hours (depending on K8s implementation scope) ✓
- Phase 5: 2 hours (architectural changes) ✓
- Phase 6: 30 minutes ✓
- Phase 7: 2-3 hours (if implemented)

**Total estimate: 6-8 hours (without Phase 7)**
**With Phase 7: 8-11 hours**