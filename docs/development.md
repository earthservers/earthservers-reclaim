# Development Guide

## Prerequisites

Before you start, make sure you have the following installed:

- **Node.js** >= 18.0.0
- **pnpm** >= 8.0.0
- **Rust** >= 1.70.0
- **Ollama** (for AI models)

### Installing Prerequisites

#### macOS
```bash
# Install Node.js
brew install node

# Install pnpm
npm install -g pnpm

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Ollama
brew install ollama
```

#### Linux
```bash
# Install Node.js (Ubuntu/Debian)
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt-get install -y nodejs

# Install pnpm
npm install -g pnpm

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Ollama
curl https://ollama.ai/install.sh | sh
```

#### Windows
```powershell
# Install Node.js - Download from https://nodejs.org/

# Install pnpm
npm install -g pnpm

# Install Rust - Download from https://rustup.rs/

# Install Ollama - Download from https://ollama.ai/download
```

## Setup

1. **Clone the repository**
```bash
git clone https://github.com/earthservers/earthservers-reclaim.git
cd earthservers-reclaim
```

2. **Install dependencies**
```bash
pnpm install
```

3. **Set up Ollama models**
```bash
# Start Ollama
ollama serve

# In another terminal, download the default models
ollama pull all-minilm  # For embeddings
ollama pull llama3.2:3b  # For text generation
```

## Development

### Running the Desktop App

```bash
# Start the Tauri app in development mode
pnpm desktop

# Or use the shorter alias
pnpm dev
```

This will:
- Start the Vite dev server for the React frontend
- Compile and run the Rust backend
- Open the desktop application window

### Running Individual Packages

```bash
# Run a specific package in dev mode
pnpm --filter @earthservers/ai-runtime dev

# Run tests for a specific package
pnpm --filter @earthservers/search-engine test
```

### Building for Production

```bash
# Build the desktop app
pnpm desktop:build

# This creates platform-specific installers in:
# apps/desktop/src-tauri/target/release/bundle/
```

## Project Structure

```
earthservers-reclaim/
├── apps/
│   └── desktop/          # Main Tauri application
│       ├── src/          # React frontend
│       └── src-tauri/    # Rust backend
│           ├── src/
│           │   ├── main.rs
│           │   ├── search.rs
│           │   ├── ai.rs
│           │   └── knowledge_graph.rs
│           └── Cargo.toml
│
├── packages/
│   ├── ai-runtime/       # Ollama integration
│   ├── search-engine/    # Domain-based search
│   ├── knowledge-graph/  # Personal memory
│   ├── database/         # SQLite schemas
│   └── ui/               # Shared React components
│
└── models/               # AI models (gitignored)
```

## Working with Tauri

### Tauri Commands

Tauri commands are Rust functions that can be called from JavaScript:

```rust
// In src-tauri/src/main.rs
#[tauri::command]
fn my_command(arg: String) -> Result<String, String> {
    Ok(format!("Received: {}", arg))
}
```

```typescript
// In React
import { invoke } from '@tauri-apps/api/tauri';

const result = await invoke('my_command', { arg: 'hello' });
```

### Debugging

- **Rust backend**: `cargo run --manifest-path apps/desktop/src-tauri/Cargo.toml`
- **React frontend**: Check browser DevTools (Ctrl+Shift+I / Cmd+Option+I)
- **Tauri logs**: View in terminal where you ran `pnpm desktop`

## Common Tasks

### Adding a New Package

```bash
# Create package directory
mkdir -p packages/my-package/src

# Create package.json
cd packages/my-package
pnpm init
```

### Adding Dependencies

```bash
# Add to specific package
pnpm --filter @earthservers/ai-runtime add axios

# Add to root (dev dependency)
pnpm add -D -w prettier
```

### Running Tests

```bash
# Run all tests
pnpm test

# Run tests for specific package
pnpm --filter @earthservers/search-engine test

# Run tests in watch mode
pnpm --filter @earthservers/search-engine test -- --watch
```

### Linting and Formatting

```bash
# Lint all packages
pnpm lint

# Format all code
pnpm format
```

## Troubleshooting

### Ollama not running
```bash
# Check if Ollama is running
curl http://localhost:11434/api/tags

# If not, start it
ollama serve
```

### Port 1420 already in use
```bash
# Find process using port
lsof -i :1420

# Kill it or change port in vite.config.ts
```

### Rust build errors
```bash
# Clean and rebuild
cd apps/desktop/src-tauri
cargo clean
cargo build
```

### pnpm install fails
```bash
# Clear cache and reinstall
pnpm store prune
rm -rf node_modules
pnpm install
```

## Architecture Decisions

### Why Monorepo?
- Shared code between services
- Atomic commits across packages
- Single CI/CD pipeline
- Easier dependency management

### Why Tauri?
- Smaller bundle size than Electron
- Better performance
- Native system integration
- Rust aligns with RISC-V roadmap

### Why Ollama?
- Easy model management
- GPU acceleration automatic
- OpenAI-compatible API
- Works offline

### Why SQLite?
- Embedded database (no server)
- Perfect for local-first apps
- Reliable and battle-tested
- Good performance for single-user

## Next Steps

1. Read [Architecture.md](./architecture.md) for system design
2. Check [User Guide](./user-guide.md) for features
3. Start coding!

## Getting Help

- **Issues**: https://github.com/earthservers/earthservers-reclaim/issues
- **Discord**: [Coming Soon]
- **Docs**: https://docs.earthservers.earth
