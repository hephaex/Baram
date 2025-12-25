# ktime - Nニュースクローラー

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

> Rustベースの高性能Nニュースクローラー + Vector DB + オントロジーシステム

## 概要

ktimeは、Nニュースから記事とコメントを収集し、ベクターデータベースに保存して、オントロジー（知識グラフ）を構築するシステムです。

### 主な機能

- **ニュースクローリング**: 政治、経済、社会、文化、世界、ITカテゴリの記事収集
- **コメント収集**: JSONP APIによるコメントと返信の再帰的収集
- **Vector DB**: OpenSearch + noriアナライザーを活用した意味ベースの検索
- **オントロジー**: LLMベースの関係抽出と知識グラフ構築
- **二重ストレージ**: SQLite（メタデータ）+ PostgreSQL 18（原本データ）

## システム要件

- Rust 1.75+
- Docker 24.0+
- PostgreSQL 18
- OpenSearch 2.11+（noriプラグイン含む）

## クイックスタート

```bash
# リポジトリをクローン
git clone https://github.com/hephaex/ktime.git
cd ktime

# 依存関係のインストールとビルド
cargo build --release

# Dockerサービスを起動
docker-compose up -d

# クローリングを実行
cargo run -- crawl --category politics --max-articles 100

# 検索
cargo run -- search "半導体投資"
```

## プロジェクト構造

```
ktime/
├── src/
│   ├── crawler/       # HTTP Fetcher、コメントクローラー
│   ├── parser/        # HTMLパーサー
│   ├── storage/       # SQLite、PostgreSQL、Markdown
│   ├── embedding/     # トークナイザー、ベクトル化
│   └── ontology/      # 関係抽出、Entity Linking
├── tests/
│   └── fixtures/      # テスト用HTML、JSONPサンプル
├── docs/
│   └── *.md           # 開発ドキュメント
└── docker/
    └── docker-compose.yml
```

## CLIコマンド

```bash
# クローリング
cargo run -- crawl --category <カテゴリ> --max-articles <数>
cargo run -- crawl --url <URL> --with-comments

# インデックス作成
cargo run -- index --input ./output/raw --batch-size 100

# 検索
cargo run -- search "検索語" --k 10

# オントロジー抽出
cargo run -- ontology --input ./output/raw --format json

# 再開
cargo run -- resume --checkpoint ./checkpoints/crawl_state.json
```

## 設定

`config.toml`ファイルで設定を管理します：

```toml
[crawler]
requests_per_second = 2
max_retries = 3

[postgresql]
host = "localhost"
port = 5432
database = "ktime"

[opensearch]
hosts = ["http://localhost:9200"]
index_name = "naver-news"
```

## ライセンス

このプロジェクトは[GPL v3ライセンス](LICENSE)に従います。

## 著作権

Copyright (c) 2025 hephaex@gmail.com

## コントリビューション

コントリビューションを歓迎します！[Issue](https://github.com/hephaex/ktime/issues)でバグレポートや機能提案をお願いします。
