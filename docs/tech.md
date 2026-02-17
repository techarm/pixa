# 画像処理ツール 技術選定書

## 前提条件

| 項目       | 選定                            |
| ---------- | ------------------------------- |
| メイン言語 | **Rust**                        |
| Web GUI    | シンプルSPA（個人・小規模向け） |
| デプロイ   | Docker + ローカル               |

---

## アーキテクチャ概要

```
┌─────────────────────────────────────────────────────────┐
│                     CLI (clap)                           │
├─────────────────────────────────────────────────────────┤
│                Core Library (lib)                        │
│  ┌──────────┬──────────┬─────────┬───────┬───────────┐ │
│  │Watermark │ Compress │ Convert │ Info  │  Prompt   │ │
│  │ Remove   │/Optimize │ Format  │Display│ Generate  │ │
│  └──────────┴──────────┴─────────┴───────┴─────┬─────┘ │
│                                                 │       │
│                                          claude / gemini │
│                                          (local CLI)     │
├─────────────────────────────────────────────────────────┤
│             Web API Server (axum)                        │
├─────────────────────────────────────────────────────────┤
│           Frontend SPA (Vanilla JS)                      │
└─────────────────────────────────────────────────────────┘
```

コアロジックを `lib` クレートに集約し、CLI と Web API の両方から呼び出す構成。

---

## 機能別 技術選定

### 1. Watermark 削除（Gemini 特化: Reverse Alpha Blending）

**採用: [GeminiWatermarkTool](https://github.com/allenk/GeminiWatermarkTool) (MIT) のアルゴリズムを Rust に移植**

| 項目    | 内容                                                                |
| ------- | ------------------------------------------------------------------- |
| 手法    | Reverse Alpha Blending（数学的逆算）                                |
| 対象    | Gemini 生成画像の可視透かし（右下の半透明ロゴ）                     |
| 精度    | ◎ 8bit 量子化誤差のみ（±1 pixel）                                   |
| AI 依存 | なし（事前キャリブレーション済みアルファマップを使用）              |
| サイズ  | 48×48（画像 ≤ 1024px） / 96×96（画像 > 1024px）を自動検出           |
| 検出    | 3段階アルゴリズム（Spatial NCC + Gradient NCC + Variance Analysis） |

**アルゴリズム:**

```
# Gemini の透かし適用:
watermarked = α × logo(255) + (1 - α) × original

# 逆算で元画像を復元:
original = (watermarked - α × 255) / (1 - α)
```

AI モデル不要のため、Docker イメージが軽量で GPU も不要。

```toml
# Cargo.toml 依存 — image クレートのみ
image = "0.25"  # 画像読み書き + ピクセル操作
```

### 2. 画像軽量化（圧縮・最適化）

| フォーマット | ライブラリ                    | 手法                                 |
| ------------ | ----------------------------- | ------------------------------------ |
| JPEG         | `mozjpeg` (via `mozjpeg-sys`) | MozJPEG エンコーダ、品質指定圧縮     |
| PNG          | `oxipng`                      | PNG 最適化（メタデータ除去、再圧縮） |
| WebP         | `webp`                        | libwebp バインディング               |
| AVIF         | `ravif`                       | rav1e ベースの AVIF エンコーダ       |
| 汎用リサイズ | `fast_image_resize`           | SIMD 最適化リサイズ                  |

**推奨構成:**

```toml
image = "0.25"              # 統一的な画像 I/O
oxipng = "9"                # PNG 最適化
mozjpeg = "0.10"            # JPEG 高圧縮
webp = "0.3"                # WebP エンコード/デコード
ravif = "0.11"              # AVIF エンコード
fast_image_resize = "4"     # 高速リサイズ
```

### 3. 画像形式変換

`image` クレートを中心に据えるのが最もシンプル。

| 変換                          | 対応                         |
| ----------------------------- | ---------------------------- |
| PNG ↔ JPEG ↔ BMP ↔ GIF ↔ TIFF | `image` クレート標準対応     |
| → WebP                        | `webp` クレート              |
| → AVIF                        | `ravif` クレート             |
| → SVG (ラスタ→ベクタ)         | `vtracer`（オプション）      |
| HEIC → 他形式                 | `libheif-rs`（システム依存） |

```rust
// 変換の基本パターン
use image::io::Reader;
let img = Reader::open("input.png")?.decode()?;
img.save("output.webp")?;  // image + webp feature
```

### 4. 画像情報表示

| 情報                             | ライブラリ                         |
| -------------------------------- | ---------------------------------- |
| 基本情報（サイズ、形式、色深度） | `image` クレート                   |
| EXIF メタデータ                  | `kamadak-exif` (= `exif` クレート) |
| ICC プロファイル                 | `lcms2` バインディング             |
| ファイルサイズ・ハッシュ         | `std::fs` + `sha2`                 |

```toml
exif = "0.5"   # EXIF 読み取り
sha2 = "0.10"  # ファイルハッシュ
```

### 5. CLI + Web GUI

#### CLI: `clap`

```toml
clap = { version = "4", features = ["derive"] }
```

```rust
/// 画像処理ツール
#[derive(Parser)]
#[command(name = "pixa")]
enum Cli {
    /// Watermark を削除
    RemoveWatermark { input: PathBuf, output: PathBuf, #[arg(long)] mask: Option<PathBuf> },
    /// 画像を軽量化
    Compress { input: PathBuf, output: PathBuf, #[arg(long, default_value = "80")] quality: u8 },
    /// 画像形式を変換
    Convert { input: PathBuf, output: PathBuf },
    /// 画像情報を表示
    Info { input: PathBuf, #[arg(long)] json: bool },
}
```

#### Web GUI

| 層                       | 選定                            | 理由                                              |
| ------------------------ | ------------------------------- | ------------------------------------------------- |
| **API サーバー**         | `axum`                          | Rust エコシステムで最も人気、tokio ベース、型安全 |
| **フロントエンド**       | **案A: Leptos（推奨）**         | Rust フルスタック、WASM SPA、型共有が可能         |
|                          | 案B: Vanilla JS + HTML          | 最小依存、ビルド不要、静的ファイル配信のみ        |
|                          | 案C: React (Vite)               | フロント経験者向け、API 経由で接続                |
| **ファイルアップロード** | `axum-multipart` / `tower-http` | ストリーミングアップロード対応                    |
| **静的ファイル配信**     | `tower-http::ServeDir`          | SPA アセットの配信                                |

**推奨: Leptos**

- Rust 統一でフロントもバックも書ける（学習コスト集中）
- 型安全な API 呼び出し
- WASM でクライアントサイドレンダリング → シンプルな SPA に最適
- ただし Leptos エコシステムの成熟度がまだ発展途上な点は要考慮

**堅実な代替案: axum + Vanilla HTML/JS**

- 追加ビルドツール不要
- `tower-http::ServeDir` で静的配信
- fetch API でバックエンドと通信
- 最もシンプル、依存最小

```toml
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["fs", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### 6. Nanobanana プロンプト生成（ローカル AI CLI 連携）

| 項目 | 内容                                                            |
| ---- | --------------------------------------------------------------- |
| 方式 | ローカルの `claude` / `gemini` CLI をサブプロセスとして呼び出し |
| 入力 | テキスト指示 / 参考画像 / スタイル・比率指定                    |
| 出力 | Nanobanana 最適化プロンプト（テキストのみ）                     |

**設計:**

- `std::process::Command` で CLI を呼び出し（外部 HTTP 通信不要）
- メタプロンプトで Nanobanana のベストプラクティスを指示
- 参考画像は CLI の画像入力機能で渡す
- 複数バリエーション生成対応

```rust
// CLI 呼び出しの基本パターン
let output = Command::new("claude")     // or "gemini"
    .arg("-p")
    .arg(&meta_prompt)
    .arg("reference.jpg")               // 参考画像（オプション）
    .output()?;
```

**メリット:**

- API キー管理不要（CLI 側で認証済み）
- Docker イメージが軽量（追加依存なし）
- `reqwest` 等の HTTP クレート不要

---

## プロジェクト構成案

```
pixa/
├── Cargo.toml              # ワークスペース定義
├── Dockerfile
├── docker-compose.yml
├── crates/
│   ├── core/               # コアライブラリ（画像処理ロジック）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── watermark.rs
│   │       ├── compress.rs
│   │       ├── convert.rs
│   │       ├── info.rs
│   │       └── prompt.rs    # プロンプト生成（claude/gemini CLI 連携）
│   ├── cli/                 # CLIバイナリ
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── web/                 # Webサーバー + SPA
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs
│       │   └── routes.rs
│       └── static/          # フロントエンドアセット
│           ├── index.html
│           ├── app.js
│           └── style.css
├── models/                  # AI モデル（LaMa ONNX 等）
│   └── lama.onnx
└── tests/
    └── fixtures/
```

---

## Docker 構成

```dockerfile
# ---- Build Stage ----
FROM rust:1.83-bookworm AS builder
RUN apt-get update && apt-get install -y \
    cmake libclang-dev libopencv-dev pkg-config
WORKDIR /app
COPY . .
RUN cargo build --release

# ---- Runtime Stage ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    libopencv-core406 libonnxruntime ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pixa-cli /usr/local/bin/
COPY --from=builder /app/target/release/pixa-web /usr/local/bin/
COPY models/ /app/models/
EXPOSE 3000
CMD ["pixa-web", "--port", "3000"]
```

---

## 依存関係まとめ

| カテゴリ     | クレート                        | 用途                 |
| ------------ | ------------------------------- | -------------------- |
| 画像 I/O     | `image`                         | 読み書き・基本変換   |
| 圧縮         | `oxipng`, `mozjpeg`, `webp`     | フォーマット別最適化 |
| リサイズ     | `fast_image_resize`             | SIMD 高速リサイズ    |
| メタデータ   | `kamadak-exif`                  | EXIF 読み取り        |
| CLI          | `clap`                          | コマンドライン引数   |
| Web          | `axum`, `tokio`, `tower-http`   | API サーバー         |
| シリアライズ | `serde`, `serde_json`           | JSON 入出力          |
| ハッシュ     | `sha2`                          | ファイル整合性       |
| ログ         | `tracing`, `tracing-subscriber` | 構造化ログ           |
| エラー       | `anyhow`, `thiserror`           | エラーハンドリング   |

---

## リスクと留意点

| リスク                                    | 対策                                               |
| ----------------------------------------- | -------------------------------------------------- |
| `mozjpeg` のネイティブビルドが環境依存    | Docker でビルド環境を固定（cmake, nasm 必須）      |
| Gemini の透かしパターンが変更される可能性 | アルファマップの更新で対応可能                     |
| HEIC 対応がシステム依存                   | `libheif` はオプション feature として切り離し      |
| Leptos の成熟度                           | 代替として Vanilla JS SPA を初期採用（現在の構成） |
| AI画像生成 API のコスト                   | レート制限 + キー管理 + 使用量モニタリング         |

---

## 次のステップ

1. **Cargo ワークスペース初期化** — `core` / `cli` / `web` の3クレート構成
2. **`image` + `clap` で MVP** — まず形式変換と情報表示を CLI で動かす
3. **圧縮機能追加** — `oxipng` / `mozjpeg` 統合
4. **axum Web サーバー** — ファイルアップロード + 処理 API
5. **Watermark 削除** — `ort` + LaMa モデル統合
6. **Docker 化** — マルチステージビルド
7. **AI 画像生成** — 外部 API 統合（オプション）
