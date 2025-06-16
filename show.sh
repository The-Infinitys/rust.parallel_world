#!/bin/bash

# ソースディレクトリ
SRC_DIR="src"

# srcディレクトリが存在するか確認
if [ ! -d "$SRC_DIR" ]; then
    echo "Error: $SRC_DIR directory not found" >&2
    exit 1
fi

# srcディレクトリ内のすべての.rsファイルを処理
find "$SRC_DIR" -type f -name "*.rs" | while read -r file; do
    # ファイルパスの出力
    echo "===========$file"
    # ファイル内容の出力
    cat "$file"
    # ファイル終了の区切り
    echo "==========="
done