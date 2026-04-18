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

echo "=== Card Data Generation ==="

# Download MTGJSON AtomicCards if not present
MTGJSON_FILE="$DATA_DIR/mtgjson/AtomicCards.json"
if [ ! -f "$MTGJSON_FILE" ]; then
  echo "Downloading MTGJSON AtomicCards..."
  mkdir -p "$DATA_DIR/mtgjson"
  curl -L -o "$MTGJSON_FILE" "https://mtgjson.com/api/v5/AtomicCards.json"
  echo "Downloaded MTGJSON data."
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
GEN_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
GEN_COMMIT=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
GEN_COMMIT_SHORT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
track_tmp "$META_OUTPUT_TMP"
cat > "$META_OUTPUT_TMP" <<METAEOF
{"generated_at":"${GEN_TIMESTAMP}","commit":"${GEN_COMMIT}","commit_short":"${GEN_COMMIT_SHORT}"}
METAEOF
promote_tmp "$META_OUTPUT_TMP" "$META_OUTPUT"
echo "Generated $META_OUTPUT"

# Summary
FILE_SIZE=$(du -h "$OUTPUT" | cut -f1)
NAMES_SIZE=$(du -h "$NAMES_OUTPUT" | cut -f1)
CARD_COUNT=$(grep -o '"name"' "$OUTPUT" | wc -l | tr -d ' ')
echo "Generated $OUTPUT ($FILE_SIZE, ~$CARD_COUNT cards)"
echo "Generated $NAMES_OUTPUT ($NAMES_SIZE)"
