#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_UNDER_TEST="${ROOT_DIR}/scripts/check-release-workflow-safety.py"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

write_good_workflow() {
  local work="$1"
  mkdir -p "${work}/.github/workflows" "${work}/packages"
  cp "${ROOT_DIR}/.github/workflows/publish-cli.yml" "${work}/.github/workflows/publish-cli.yml"
  for package_dir in \
    cli-darwin-arm64 \
    cli-darwin-x64 \
    cli-linux-x64-gnu \
    cli-linux-x64-musl \
    cli-linux-arm64-gnu \
    cli-linux-arm64-musl \
    cli-win32-x64-msvc \
    cli-win32-arm64-msvc; do
    mkdir -p "${work}/packages/${package_dir}"
    cp "${ROOT_DIR}/packages/${package_dir}/package.json" "${work}/packages/${package_dir}/package.json"
  done
}

run_check() {
  local work="$1"
  local output="$2"
  (cd "${work}" && python3 "${SCRIPT_UNDER_TEST}" >"${output}" 2>&1)
}

test_accepts_release_workflow() {
  local work="${TMP_DIR}/good"
  write_good_workflow "${work}"
  run_check "${work}" "${TMP_DIR}/good-output.txt"
  grep -q "Release workflow safety OK" "${TMP_DIR}/good-output.txt"
}

test_reads_workflow_as_utf8_when_locale_is_non_utf8() {
  local work="${TMP_DIR}/utf8-locale"
  write_good_workflow "${work}"
  printf '# UTF-8 sentinel: 🧪\n' >> "${work}/.github/workflows/publish-cli.yml"
  (
    cd "${work}"
    LC_ALL=C PYTHONUTF8=0 python3 "${SCRIPT_UNDER_TEST}" >"${TMP_DIR}/utf8-locale-output.txt" 2>&1
  )
  grep -q "Release workflow safety OK" "${TMP_DIR}/utf8-locale-output.txt"
}

test_rejects_build_matrix_target_drift() {
  local work="${TMP_DIR}/target-drift"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = text.replace("target: x86_64-unknown-linux-gnu", "target: riscv64gc-unknown-linux-gnu", 1)
path.write_text(text)
PY

  local output="${TMP_DIR}/target-drift-output.txt"
  if run_check "${work}" "${output}"; then
    echo "Expected workflow safety check to reject target drift" >&2
    return 1
  fi
  grep -q "build matrix targets differ from supported targets" "${output}"
}

test_rejects_platform_publish_matrix_drift() {
  local work="${TMP_DIR}/publish-drift"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
marker = "  publish-platform-packages:"
before, after = text.split(marker, 1)
after = after.replace(
    "artifact_name: cli-binary-x86_64-unknown-linux-gnu",
    "artifact_name: cli-binary-x86_64-unknown-linux-musl",
    1,
)
path.write_text(before + marker + after)
PY

  local output="${TMP_DIR}/publish-drift-output.txt"
  if run_check "${work}" "${output}"; then
    echo "Expected workflow safety check to reject platform publish drift" >&2
    return 1
  fi
  grep -q "publish platform artifact drift" "${output}"
}

test_rejects_missing_release_artifact_smoke_job() {
  local work="${TMP_DIR}/missing-smoke"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = re.sub(r"\n  smoke-release-artifacts:\n(?:    .*\n|\n)*?(?=  prepare-release-provenance:)", "\n", text)
path.write_text(text)
PY

  local output="${TMP_DIR}/missing-smoke-output.txt"
  if run_check "${work}" "${output}"; then
    echo "Expected workflow safety check to reject missing release artifact smoke job" >&2
    return 1
  fi
  grep -q "publish workflow missing smoke-release-artifacts job" "${output}"
}

test_rejects_commented_release_artifact_smoke_requirements() {
  local work="${TMP_DIR}/commented-smoke-requirements"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()
text = text.replace("          pattern: cli-binary-*", "          # pattern: cli-binary-*")
text = text.replace("        run: bash scripts/test-release-package-artifacts.sh", "        # run: bash scripts/test-release-package-artifacts.sh")
path.write_text(text)
PY

  local output="${TMP_DIR}/commented-smoke-requirements-output.txt"
  if run_check "${work}" "${output}"; then
    echo "Expected workflow safety check to reject commented smoke requirements" >&2
    return 1
  fi
  grep -Fq "smoke-release-artifacts job must download cli-binary-* artifacts" "${output}"
  grep -q "smoke-release-artifacts job must run scripts/test-release-package-artifacts.sh" "${output}"
}

test_accepts_multiline_release_artifact_smoke_dependency() {
  local work="${TMP_DIR}/multiline-smoke-need"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text().replace(
    "needs: [bump-versions, build-cli-binary, smoke-release-artifacts]",
    "needs:\n      - bump-versions\n      - build-cli-binary\n      - smoke-release-artifacts",
)
path.write_text(text)
PY
  run_check "${work}" "${TMP_DIR}/multiline-smoke-need-output.txt"
  grep -q "Release workflow safety OK" "${TMP_DIR}/multiline-smoke-need-output.txt"
}

test_rejects_provenance_without_release_artifact_smoke_dependency() {
  local work="${TMP_DIR}/missing-smoke-need"
  write_good_workflow "${work}"
  python3 - "${work}/.github/workflows/publish-cli.yml" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text().replace(
    "needs: [bump-versions, build-cli-binary, smoke-release-artifacts]",
    "needs: [bump-versions, build-cli-binary]",
)
path.write_text(text)
PY

  local output="${TMP_DIR}/missing-smoke-need-output.txt"
  if run_check "${work}" "${output}"; then
    echo "Expected workflow safety check to reject missing smoke dependency" >&2
    return 1
  fi
  grep -q "prepare-release-provenance must depend on smoke-release-artifacts" "${output}"
}

test_accepts_release_workflow
test_reads_workflow_as_utf8_when_locale_is_non_utf8
test_rejects_build_matrix_target_drift
test_rejects_platform_publish_matrix_drift
test_rejects_missing_release_artifact_smoke_job
test_rejects_commented_release_artifact_smoke_requirements
test_accepts_multiline_release_artifact_smoke_dependency
test_rejects_provenance_without_release_artifact_smoke_dependency

echo "release workflow safety tests passed"
