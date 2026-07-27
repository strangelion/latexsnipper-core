#!/usr/bin/env sh
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: run_capture.sh MODEL_DIR FEED_NPZ OUTPUT_JSON" >&2
  exit 64
fi

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
python_bin=${PYTHON:-python3}
venv_dir=$(mktemp -d "${TMPDIR:-/tmp}/latexsnipper-decoder.XXXXXX")
trap 'rm -rf -- "$venv_dir"' EXIT HUP INT TERM

"$python_bin" -m venv "$venv_dir"
"$venv_dir/bin/python" -m pip install --requirement "$script_dir/requirements-paddle.txt"
"$venv_dir/bin/python" "$script_dir/capture_paddle_while_state.py" \
  --model-dir "$1" \
  --feed-npz "$2" \
  --output "$3"
