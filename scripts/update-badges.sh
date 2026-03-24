#!/usr/bin/env bash
# Update README.md coverage badges from coverage-stats.json
# Usage: ./scripts/update-badges.sh [path-to-stats-json]
set -euo pipefail

STATS_FILE="${1:-data/coverage-stats.json}"
README="README.md"

if [ ! -f "$STATS_FILE" ]; then
  echo "Error: $STATS_FILE not found" >&2
  exit 1
fi

# Read stats with jq
coverage_pct=$(jq -r '.coverage_pct' "$STATS_FILE")
supported=$(jq -r '.supported_cards' "$STATS_FILE")
total=$(jq -r '.total_cards' "$STATS_FILE")
keywords=$(jq -r '.keyword_count' "$STATS_FILE")

# Format badge color based on percentage
badge_color() {
  local pct=$1
  if [ "$pct" -ge 90 ]; then echo "brightgreen"
  elif [ "$pct" -ge 80 ]; then echo "green"
  elif [ "$pct" -ge 70 ]; then echo "yellowgreen"
  else echo "yellow"
  fi
}

coverage_int=${coverage_pct%.*}
overall_color=$(badge_color "$coverage_int")

# Build format badge lines (sorted by pct descending)
format_badges=$(jq -r '
  .formats | to_entries
  | map(select(.key | test("^(pauper|standard|pioneer|modern|legacy|commander|vintage)$")))
  | sort_by(-.value.pct)
  | .[] | "\(.key):\(.value.pct)"
' "$STATS_FILE")

format_lines=""
while IFS=: read -r fmt pct; do
  # Capitalize format name
  label="$(echo "${fmt:0:1}" | tr '[:lower:]' '[:upper:]')${fmt:1}"
  color=$(badge_color "$pct")
  format_lines="${format_lines}  <img alt=\"${label}\" src=\"https://img.shields.io/badge/${label}-${pct}%25-${color}\">"$'\n'
done <<< "$format_badges"

# Build the replacement block
new_block="<!-- coverage-badges:start -->
<p align=\"center\">
  <img alt=\"Card Coverage\" src=\"https://img.shields.io/badge/card_coverage-${coverage_int}%25-${overall_color}\">
  <img alt=\"Keywords\" src=\"https://img.shields.io/badge/keywords-${keywords}%2F${keywords}-brightgreen\">
  <img alt=\"Cards\" src=\"https://img.shields.io/badge/cards-${supported}%2F${total}-${overall_color}\">
  <br/>
${format_lines}</p>
<!-- coverage-badges:end -->"

# Replace the block between markers in README
python3 -c "
import re, sys
readme = open('$README').read()
pattern = r'<!-- coverage-badges:start -->.*?<!-- coverage-badges:end -->'
replacement = sys.stdin.read()
updated = re.sub(pattern, replacement, readme, flags=re.DOTALL)
open('$README', 'w').write(updated)
" <<< "$new_block"

echo "Updated $README badges: ${coverage_int}% coverage, ${supported}/${total} cards, ${keywords} keywords"
