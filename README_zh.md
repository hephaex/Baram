# nTimes - N新闻爬虫

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org/)

> 基于Rust的高性能N新闻爬虫 + Vector DB + 本体系统

## 概述

nTimes是一个从N新闻收集文章和评论，存储到向量数据库，并构建本体（知识图谱）的系统。

### 主要功能

- **新闻爬取**: 收集政治、经济、社会、文化、世界、IT类别的文章
- **评论收集**: 通过JSONP API递归收集评论和回复
- **Vector DB**: 利用OpenSearch + nori分析器进行语义搜索
- **本体**: 基于LLM的关系提取和知识图谱构建
- **双重存储**: SQLite（元数据）+ PostgreSQL 18（原始数据）

## 系统要求

- Rust 1.75+
- Docker 24.0+
- PostgreSQL 18
- OpenSearch 2.11+（包含nori插件）

## 快速开始

```bash
# 克隆仓库
git clone https://github.com/hephaex/nTimes.git
cd nTimes

# 安装依赖并构建
cargo build --release

# 启动Docker服务
docker-compose up -d

# 执行爬取
cargo run -- crawl --category politics --max-articles 100

# 搜索
cargo run -- search "半导体投资"
```

## 项目结构

```
nTimes/
├── src/
│   ├── crawler/       # HTTP Fetcher、评论爬虫
│   ├── parser/        # HTML解析器
│   ├── storage/       # SQLite、PostgreSQL、Markdown
│   ├── embedding/     # 分词器、向量化
│   └── ontology/      # 关系提取、实体链接
├── tests/
│   └── fixtures/      # 测试用HTML、JSONP样本
├── docs/
│   └── *.md           # 开发文档
└── docker/
    └── docker-compose.yml
```

## CLI命令

```bash
# 爬取
cargo run -- crawl --category <类别> --max-articles <数量>
cargo run -- crawl --url <URL> --with-comments

# 索引
cargo run -- index --input ./output/raw --batch-size 100

# 搜索
cargo run -- search "搜索词" --k 10

# 本体提取
cargo run -- ontology --input ./output/raw --format json

# 恢复
cargo run -- resume --checkpoint ./checkpoints/crawl_state.json
```

## 配置

通过`config.toml`文件管理配置：

```toml
[crawler]
requests_per_second = 2
max_retries = 3

[postgresql]
host = "localhost"
port = 5432
database = "ntimes"

[opensearch]
hosts = ["http://localhost:9200"]
index_name = "naver-news"
```

## 许可证

本项目遵循[GPL v3许可证](LICENSE)。

## 版权

Copyright (c) 2025 hephaex@gmail.com

## 贡献

欢迎贡献！请通过[Issue](https://github.com/hephaex/nTimes/issues)提交bug报告或功能建议。
