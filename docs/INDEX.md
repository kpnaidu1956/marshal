# Ruvector Documentation Index

Complete index of all Ruvector documentation.

## Quick Links

- [Getting Started](guide/GETTING_STARTED.md) - Start here!
- [Installation](guide/INSTALLATION.md) - Platform-specific installation
- [API Reference](api/) - Complete API documentation
- [Examples](../examples/) - Working code examples
- [Contributing](CONTRIBUTING.md) - How to contribute

## User Guides

### Getting Started
- **[Getting Started Guide](guide/GETTING_STARTED.md)** - Quick introduction to Ruvector
- **[Installation Guide](guide/INSTALLATION.md)** - Installation for Rust, Node.js, WASM, CLI
- **[Basic Tutorial](guide/BASIC_TUTORIAL.md)** - Step-by-step tutorial with examples
- **[Advanced Features Guide](guide/ADVANCED_FEATURES.md)** - Hybrid search, quantization, MMR, filtering

### Migration
- **[Migration from AgenticDB](MIGRATION.md)** - Complete migration guide with examples

## Architecture Documentation

- **[System Overview](architecture/SYSTEM_OVERVIEW.md)** - High-level architecture and design
  - Storage Layer (redb, memmap2, rkyv)
  - Index Layer (HNSW, Flat)
  - Query Engine (SIMD, parallel execution)
  - Multi-platform bindings

## API Reference

### Platform APIs
- **[Rust API](api/RUST_API.md)** - Complete Rust API reference
  - VectorDB
  - AgenticDB (5-table schema)
  - Types and configuration
  - Advanced features
  - Error handling

- **[Node.js API](api/NODEJS_API.md)** - Complete Node.js API reference
  - VectorDB class
  - AgenticDB class
  - TypeScript types
  - Examples

### Feature-Specific APIs
- **[AgenticDB API](AGENTICDB_API.md)** - Detailed AgenticDB API documentation
  - Reflexion Memory
  - Skill Library
  - Causal Memory
  - Learning Sessions
  - 9 RL algorithms

- **[WASM API](wasm-api.md)** - Browser WASM API
- **[WASM Build Guide](wasm-build-guide.md)** - Building for WASM

## Examples

### Rust Examples
- **[basic_usage.rs](../examples/rust/basic_usage.rs)** - Basic insert and search
- **[batch_operations.rs](../examples/rust/batch_operations.rs)** - High-throughput batch operations
- **[rag_pipeline.rs](../examples/rust/rag_pipeline.rs)** - Complete RAG implementation
- **[agenticdb_demo.rs](../examples/agenticdb_demo.rs)** - All AgenticDB features
- **[advanced_features.rs](../examples/advanced_features.rs)** - Hybrid search, MMR, filtering

### Node.js Examples
- **[basic_usage.js](../examples/nodejs/basic_usage.js)** - Basic Node.js usage
- **[semantic_search.js](../examples/nodejs/semantic_search.js)** - Semantic search application

### WASM Examples
- **[Vanilla JS](../examples/wasm-vanilla/)** - Pure JavaScript WASM example
- **[React](../examples/wasm-react/)** - React application with WASM

## Performance & Benchmarks

- **[Benchmarking Guide](benchmarks/BENCHMARKING_GUIDE.md)** - How to run and interpret benchmarks
  - Distance metrics benchmarks
  - HNSW search benchmarks
  - Batch operations benchmarks
  - Quantization benchmarks
  - Comparison methodology
  - Performance targets

### Optimization Guides
- **[Performance Tuning Guide](optimization/PERFORMANCE_TUNING_GUIDE.md)** - Detailed optimization guide
- **[Build Optimization](optimization/BUILD_OPTIMIZATION.md)** - Compilation optimizations
- **[Optimization Results](optimization/OPTIMIZATION_RESULTS.md)** - Benchmark results

## Implementation Documentation

### Phase Summaries
- **[Phase 2: HNSW Implementation](phase2_hnsw_implementation.md)** - HNSW integration details
- **[Phase 3: AgenticDB](PHASE3_SUMMARY.md)** - AgenticDB compatibility layer
- **[Phase 4: Advanced Features](phase4-implementation-summary.md)** - Product quantization, hybrid search
- **[Phase 5: Multi-Platform](phase5-implementation-summary.md)** - Node.js, WASM, CLI
- **[Phase 6: Advanced Techniques](PHASE6_SUMMARY.md)** - Future-oriented features

### Development Guides
- **[Contributing Guide](CONTRIBUTING.md)** - How to contribute to Ruvector
  - Code style guidelines
  - Testing requirements
  - PR process
  - Commit guidelines
  - Performance considerations

- **[Test Suite Summary](TDD_TEST_SUITE_SUMMARY.md)** - Testing strategy and coverage

## Project Information

- **[README](../README.md)** - Project overview and technical plan
- **[CHANGELOG](../CHANGELOG.md)** - Version history and changes
- **[LICENSE](../LICENSE)** - MIT License

## Documentation Statistics

- **Total documentation files**: 28+ markdown files
- **Total documentation lines**: 12,870+ lines
- **User guides**: 4 comprehensive guides
- **API references**: 3 platform APIs
- **Code examples**: 7+ working examples
- **Languages covered**: Rust, JavaScript/TypeScript, WASM

## Getting Help

### Resources
- **Documentation**: This index and linked guides
- **Examples**: [../examples/](../examples/) directory
- **API docs**: `cargo doc --no-deps --open`
- **Benchmarks**: `cargo bench`

### Support Channels
- **GitHub Issues**: [Report bugs or request features](https://github.com/ruvnet/ruvector/issues)
- **GitHub Discussions**: [Ask questions](https://github.com/ruvnet/ruvector/discussions)
- **Pull Requests**: [Contribute code](https://github.com/ruvnet/ruvector/pulls)

## Documentation Roadmap

### Completed ✅
- ✅ Getting Started guides
- ✅ Installation for all platforms
- ✅ Basic and advanced tutorials
- ✅ Complete API reference
- ✅ Architecture documentation
- ✅ Benchmarking guide
- ✅ Contributing guide
- ✅ Migration guide
- ✅ Multiple working examples

### Planned for Future Versions
- 📝 Video tutorials
- 📝 Interactive examples
- 📝 Performance case studies
- 📝 Advanced architecture deep-dives
- 📝 Troubleshooting cookbook
- 📝 Production deployment guide
- 📝 Monitoring and observability guide

## Contributing to Documentation

We welcome documentation contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Documentation Style Guide

1. **Clear and concise**: Use simple language
2. **Code examples**: Include working examples
3. **Step-by-step**: Break complex topics into steps
4. **Cross-references**: Link to related documentation
5. **Updates**: Keep documentation in sync with code

### Reporting Documentation Issues

Found an error or gap in documentation?
1. Check if it's already reported in [GitHub Issues](https://github.com/ruvnet/ruvector/issues)
2. Open a new issue with the "documentation" label
3. Describe the problem clearly
4. Suggest improvements if possible

---

**Last Updated**: 2025-11-19
**Version**: 0.1.0
