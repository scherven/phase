#!/usr/bin/env bash
set -euo pipefail

# Load .env if present (for PHASE_FORGE_PATH, etc.)
if [ -f ".env" ]; then
  set -a; source .env; set +a
fi

DATA_DIR="data"
OUTPUT_DIR="client/public"
OUTPUT="${OUTPUT_DIR}/card-data.json"
NAMES_OUTPUT="${OUTPUT_DIR}/card-names.json"
COVERAGE_OUTPUT="${OUTPUT_DIR}/coverage-data.json"
COVERAGE_SUMMARY="${OUTPUT_DIR}/coverage-summary.json"
META_OUTPUT="${OUTPUT_DIR}/card-data-meta.json"
SET_LIST_OUTPUT="${OUTPUT_DIR}/set-list.json"
DECKS_OUTPUT="${OUTPUT_DIR}/decks.json"

echo "=== Card Data Generation ==="

# Download MTGJSON AtomicCards if not present
MTGJSON_FILE="$DATA_DIR/mtgjson/AtomicCards.json"
if [ ! -f "$MTGJSON_FILE" ]; then
  echo "Downloading MTGJSON AtomicCards..."
  mkdir -p "$DATA_DIR/mtgjson"
  curl -L -o "$MTGJSON_FILE" "https://mtgjson.com/api/v5/AtomicCards.json"
  echo "Downloaded MTGJSON data."
fi

# Download ancillary MTGJSON sidecar files (small, cheap to refresh)
MTGJSON_META_FILE="$DATA_DIR/mtgjson/Meta.json"
if [ ! -f "$MTGJSON_META_FILE" ]; then
  echo "Downloading MTGJSON Meta..."
  mkdir -p "$DATA_DIR/mtgjson"
  curl -L -o "$MTGJSON_META_FILE" "https://mtgjson.com/api/v5/Meta.json"
fi

MTGJSON_SET_LIST_FILE="$DATA_DIR/mtgjson/SetList.json"
if [ ! -f "$MTGJSON_SET_LIST_FILE" ]; then
  echo "Downloading MTGJSON SetList..."
  mkdir -p "$DATA_DIR/mtgjson"
  curl -L -o "$MTGJSON_SET_LIST_FILE" "https://mtgjson.com/api/v5/SetList.json"
fi

# AllDeckFiles is shipped as a tarball of per-deck JSONs. Extract to
# data/mtgjson/decks/ once; refreshing means deleting the directory.
MTGJSON_DECKS_DIR="$DATA_DIR/mtgjson/decks"
if [ ! -d "$MTGJSON_DECKS_DIR" ]; then
  echo "Downloading MTGJSON AllDeckFiles..."
  mkdir -p "$MTGJSON_DECKS_DIR"
  MTGJSON_DECKS_ARCHIVE="$DATA_DIR/mtgjson/AllDeckFiles.tar.gz"
  curl -L -o "$MTGJSON_DECKS_ARCHIVE" "https://mtgjson.com/api/v5/AllDeckFiles.tar.gz"
  tar -xzf "$MTGJSON_DECKS_ARCHIVE" -C "$MTGJSON_DECKS_DIR" --strip-components=1
  rm -f "$MTGJSON_DECKS_ARCHIVE"
fi

# Build and run the Oracle-based card data generator
echo "Generating card data from MTGJSON via Oracle text parser..."
mkdir -p "$(dirname "$OUTPUT")"

# Enable Forge bridge when cardsfolder is available
FEATURES="cli"
if [ -n "${PHASE_FORGE_PATH:-}" ] || [ -d "$DATA_DIR/forge-cardsfolder" ]; then
  FEATURES="cli,forge"
  echo "Forge bridge enabled"
fi

# Write to a .tmp sibling first, and only promote to the final path on success.
# Groups that validate are promoted eagerly, so a failure in one pipeline
# stage (e.g. coverage-report) does not wipe already-validated outputs from
# an earlier stage (e.g. the expensive oracle-gen card-data + names).
PENDING_TMP=()
cleanup_tmp() {
  # `${arr[@]+"${arr[@]}"}` is the bash-3.2-safe way to expand a
  # possibly-empty array under `set -u` (macOS default is bash 3.2).
  local f
  for f in ${PENDING_TMP[@]+"${PENDING_TMP[@]}"}; do
    [ -e "$f" ] && rm -f "$f"
  done
}
trap cleanup_tmp EXIT

# Add a .tmp path to the pending-cleanup list.
track_tmp() {
  PENDING_TMP+=("$1")
}

# Atomically rename tmp → final and remove the path from the pending list
# so the EXIT trap won't touch the now-promoted file.
promote_tmp() {
  local tmp="$1"
  local final="$2"
  mv -f "$tmp" "$final"
  local i
  local new=()
  for i in ${PENDING_TMP[@]+"${PENDING_TMP[@]}"}; do
    [ "$i" = "$tmp" ] || new+=("$i")
  done
  PENDING_TMP=(${new[@]+"${new[@]}"})
}

run_tool_with_recovery() {
  local output_file="$1"
  shift

  if "$@" > "$output_file"; then
    return 0
  fi

  echo "Tool profile build failed; clearing target/tool and retrying once..." >&2
  rm -rf target/tool
  "$@" > "$output_file"
}

OUTPUT_TMP="${OUTPUT}.tmp"
NAMES_OUTPUT_TMP="${NAMES_OUTPUT}.tmp"
COVERAGE_OUTPUT_TMP="${COVERAGE_OUTPUT}.tmp"
COVERAGE_SUMMARY_TMP="${COVERAGE_SUMMARY}.tmp"
META_OUTPUT_TMP="${META_OUTPUT}.tmp"

# --- Group 1: card-data + card-names (expensive, independent of coverage) ---
track_tmp "$OUTPUT_TMP"
track_tmp "$NAMES_OUTPUT_TMP"
run_tool_with_recovery \
  "$OUTPUT_TMP" \
  cargo run --profile tool --bin oracle-gen --features "$FEATURES" -- "$DATA_DIR" --stats --names-out "$NAMES_OUTPUT_TMP"
if [ ! -s "$OUTPUT_TMP" ] || ! jq -e 'type == "object" and length > 0' "$OUTPUT_TMP" >/dev/null 2>&1; then
  echo "Generated $OUTPUT_TMP is empty or not a valid card object; aborting." >&2
  exit 1
fi
if [ ! -s "$NAMES_OUTPUT_TMP" ] || ! jq -e '.' "$NAMES_OUTPUT_TMP" >/dev/null 2>&1; then
  echo "Generated $NAMES_OUTPUT_TMP is empty or not valid JSON; aborting." >&2
  exit 1
fi
# Promote immediately — coverage-report failure below must NOT invalidate this.
promote_tmp "$OUTPUT_TMP"       "$OUTPUT"
promote_tmp "$NAMES_OUTPUT_TMP" "$NAMES_OUTPUT"
echo "Promoted $OUTPUT and $NAMES_OUTPUT"

# --- Group 2: coverage-data + coverage-summary (best-effort sidecar) ---
# A coverage-report failure warns but does not fail the whole pipeline — the
# main card-data has already been promoted above.
echo "Generating card coverage data..."
track_tmp "$COVERAGE_OUTPUT_TMP"
track_tmp "$COVERAGE_SUMMARY_TMP"
coverage_ok=1
if ! run_tool_with_recovery "$COVERAGE_OUTPUT_TMP" \
      cargo run --profile tool --bin coverage-report -- "$DATA_DIR" --all; then
  echo "WARNING: coverage-report failed; leaving existing $COVERAGE_OUTPUT in place." >&2
  coverage_ok=0
elif [ ! -s "$COVERAGE_OUTPUT_TMP" ] || ! jq -e '.' "$COVERAGE_OUTPUT_TMP" >/dev/null 2>&1; then
  echo "WARNING: $COVERAGE_OUTPUT_TMP is empty or not valid JSON; leaving existing $COVERAGE_OUTPUT in place." >&2
  coverage_ok=0
fi
if [ "$coverage_ok" = 1 ]; then
  if ! jq '{total_cards, supported_cards, coverage_pct, coverage_by_format}' \
        "$COVERAGE_OUTPUT_TMP" > "$COVERAGE_SUMMARY_TMP"; then
    echo "WARNING: coverage-summary derivation failed; leaving existing $COVERAGE_SUMMARY in place." >&2
  else
    promote_tmp "$COVERAGE_OUTPUT_TMP"  "$COVERAGE_OUTPUT"
    promote_tmp "$COVERAGE_SUMMARY_TMP" "$COVERAGE_SUMMARY"
    echo "Promoted $COVERAGE_OUTPUT and $COVERAGE_SUMMARY"
  fi
fi

# --- Group 3: metadata sidecar (cheap, always safe to update) ---
# Folds MTGJSON's Meta.json (version + date) into the same file so the frontend
# has one source of truth for "which snapshot was this card-data.json built from".
GEN_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
GEN_COMMIT=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
GEN_COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
MTGJSON_VERSION="unknown"
MTGJSON_DATE="unknown"
if [ -s "$MTGJSON_META_FILE" ]; then
  MTGJSON_VERSION=$(jq -r '.meta.version // "unknown"' "$MTGJSON_META_FILE")
  MTGJSON_DATE=$(jq -r '.meta.date // "unknown"' "$MTGJSON_META_FILE")
fi
track_tmp "$META_OUTPUT_TMP"
cat > "$META_OUTPUT_TMP" <<METAEOF
{"generated_at":"${GEN_TIMESTAMP}","commit":"${GEN_COMMIT}","commit_short":"${GEN_COMMIT_SHORT}","mtgjson_version":"${MTGJSON_VERSION}","mtgjson_date":"${MTGJSON_DATE}"}
METAEOF
promote_tmp "$META_OUTPUT_TMP" "$META_OUTPUT"
echo "Generated $META_OUTPUT"

# --- Group 4: set-list projection (best-effort sidecar) ---
SET_LIST_OUTPUT_TMP="${SET_LIST_OUTPUT}.tmp"
track_tmp "$SET_LIST_OUTPUT_TMP"
if cargo run --profile tool --bin oracle-gen --features "$FEATURES" -- \
     set-list "$DATA_DIR" "$SET_LIST_OUTPUT_TMP"; then
  if jq -e 'type == "object" and length > 0' "$SET_LIST_OUTPUT_TMP" >/dev/null 2>&1; then
    promote_tmp "$SET_LIST_OUTPUT_TMP" "$SET_LIST_OUTPUT"
    echo "Promoted $SET_LIST_OUTPUT"
  else
    echo "WARNING: $SET_LIST_OUTPUT_TMP is empty or invalid; leaving existing $SET_LIST_OUTPUT in place." >&2
  fi
else
  echo "WARNING: set-list projection failed; leaving existing $SET_LIST_OUTPUT in place." >&2
fi

# --- Group 5: preconstructed decks (best-effort sidecar) ---
# Filters MTGJSON's preconstructed decks to those whose every card the engine
# can run right now. Always emits the debug `--emit-skipped` sidecar in dev
# builds so parser coverage gaps surface as "decks blocked by card X".
DECKS_OUTPUT_TMP="${DECKS_OUTPUT}.tmp"
track_tmp "$DECKS_OUTPUT_TMP"
if cargo run --profile tool --bin oracle-gen --features "$FEATURES" -- \
     decks "$DATA_DIR" "$DECKS_OUTPUT_TMP" --emit-skipped; then
  if jq -e 'type == "object"' "$DECKS_OUTPUT_TMP" >/dev/null 2>&1; then
    promote_tmp "$DECKS_OUTPUT_TMP" "$DECKS_OUTPUT"
    echo "Promoted $DECKS_OUTPUT"
  else
    echo "WARNING: $DECKS_OUTPUT_TMP is invalid; leaving existing $DECKS_OUTPUT in place." >&2
  fi
else
  echo "WARNING: decks projection failed; leaving existing $DECKS_OUTPUT in place." >&2
fi

# Summary
FILE_SIZE=$(du -h "$OUTPUT" | cut -f1)
NAMES_SIZE=$(du -h "$NAMES_OUTPUT" | cut -f1)
CARD_COUNT=$(grep -o '"name"' "$OUTPUT" | wc -l | tr -d ' ')
echo "Generated $OUTPUT ($FILE_SIZE, ~$CARD_COUNT cards)"
echo "Generated $NAMES_OUTPUT ($NAMES_SIZE)"
