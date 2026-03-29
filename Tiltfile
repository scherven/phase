# phase.rs — local development orchestration
#
# Usage:
#   tilt up                              core dev loop (wasm + frontend)
#   tilt up -- server                    also start the game server
#   tilt up -- test lint                 also start test runners and linters
#   tilt up -- server test lint          full stack
#   tilt up -- tauri                     desktop app (replaces frontend)
#
# All resources are always visible in the Tilt UI — opt-in groups just
# control which auto-start. Click any stopped resource to start it on demand.

config.define_string_list('enable', args = True, usage = 'Resource groups to auto-start: server, tauri, test, lint')
enabled = config.parse().get('enable', [])

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

ENGINE_SRC = ['crates/engine/src/']
AI_SRC = ['crates/phase-ai/src/']
WASM_SRC = ['crates/engine-wasm/src/']

local_resource('wasm',
    cmd = './scripts/build-wasm.sh',
    deps = ENGINE_SRC + AI_SRC + WASM_SRC,
    labels = ['build'],
)

# ---------------------------------------------------------------------------
# Serve
# ---------------------------------------------------------------------------

local_resource('frontend',
    serve_cmd = 'pnpm dev',
    serve_dir = 'client',
    resource_deps = ['wasm'],
    auto_init = 'tauri' not in enabled,
    links = ['http://localhost:5173'],
    labels = ['serve'],
)

TAURI_SRC = ['client/src-tauri/src/']
SIDECAR_DEST = 'client/src-tauri/binaries/phase-server-' + str(local('rustc -vV | sed -n "s/host: //p" | tr -d "\\n"', quiet = True))

local_resource('tauri',
    cmd = 'cargo build -p phase-server && mkdir -p client/src-tauri/binaries && cp target/debug/phase-server ' + SIDECAR_DEST,
    serve_cmd = 'pnpm tauri:dev',
    serve_dir = 'client',
    deps = ENGINE_SRC + AI_SRC + WASM_SRC + TAURI_SRC + ['crates/server-core/src/', 'crates/phase-server/src/'],
    auto_init = 'tauri' in enabled,
    labels = ['serve'],
)

SERVER_SRC = ENGINE_SRC + AI_SRC + [
    'crates/server-core/src/',
    'crates/phase-server/src/',
]

local_resource('server',
    cmd = 'cargo build --bin phase-server',
    serve_cmd = './target/debug/phase-server',
    serve_env = {'PHASE_DATA_DIR': 'data'},
    deps = SERVER_SRC,
    auto_init = 'server' in enabled,
    links = ['http://localhost:9374'],
    labels = ['serve'],
)

# ---------------------------------------------------------------------------
# Test
# ---------------------------------------------------------------------------

local_resource('test-engine',
    cmd = 'cargo test -p engine',
    deps = ENGINE_SRC,
    auto_init = 'test' in enabled,
    labels = ['test'],
)

local_resource('test-frontend',
    cmd = 'pnpm test -- --run',
    dir = 'client',
    deps = ['client/src/'],
    resource_deps = ['wasm'],
    auto_init = 'test' in enabled,
    labels = ['test'],
)

# ---------------------------------------------------------------------------
# Lint
# ---------------------------------------------------------------------------

local_resource('clippy',
    cmd = 'cargo clippy --all-targets -- -D warnings',
    deps = ['crates/'],
    auto_init = 'lint' in enabled,
    labels = ['lint'],
)

local_resource('check-frontend',
    cmd = 'pnpm run type-check && pnpm lint',
    dir = 'client',
    deps = ['client/src/'],
    resource_deps = ['wasm'],
    auto_init = 'lint' in enabled,
    labels = ['lint'],
)

# ---------------------------------------------------------------------------
# Data (manual trigger — click in UI to run)
# ---------------------------------------------------------------------------

local_resource('card-data',
    cmd = './scripts/gen-card-data.sh',
    trigger_mode = TRIGGER_MODE_MANUAL,
    auto_init = False,
    labels = ['data'],
)

local_resource('coverage',
    cmd = 'cargo coverage',
    trigger_mode = TRIGGER_MODE_MANUAL,
    auto_init = False,
    labels = ['data'],
)
