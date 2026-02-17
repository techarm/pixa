# pixa 🖼️

Rust 製の高速画像処理ツールキット。CLI と Web GUI の両方に対応。

## 機能

| 機能               | 説明                                                               |
| ------------------ | ------------------------------------------------------------------ |
| **Watermark 削除** | Gemini AI の透かしを Reverse Alpha Blending で数学的に除去         |
| **画像軽量化**     | MozJPEG / OxiPNG / WebP による高品質圧縮                           |
| **形式変換**       | JPEG ↔ PNG ↔ WebP ↔ BMP ↔ GIF ↔ TIFF                               |
| **情報表示**       | 寸法・カラー・EXIF・SHA-256 等を一覧                               |
| **プロンプト生成** | ローカル AI CLI (claude/gemini) で Nanobanana 向けプロンプトを生成 |

> Watermark 削除アルゴリズムは [GeminiWatermarkTool](https://github.com/allenk/GeminiWatermarkTool) by Allen Kuo (MIT License) を参考にしています。

## クイックスタート

### CLI

```bash
# Watermark 削除
pixa remove-watermark image.jpg -o clean.jpg

# Watermark 検出
pixa detect image.jpg

# 画像圧縮
pixa compress input.png -o output.png -q 80

# 形式変換
pixa convert photo.png output.webp

# 画像情報
pixa info photo.jpg --json

# バッチ処理
pixa batch ./input/ -o ./output/ --operation remove-watermark

# プロンプト生成（テキストから）
pixa prompt "猫が宇宙で浮いてる絵" --provider claude

# プロンプト生成（参考画像から）
pixa prompt --image reference.jpg --provider gemini

# プロンプト生成（テキスト + 参考画像 + スタイル指定、3バリエーション）
pixa prompt "サイバーパンクな東京" --image ref.jpg --style anime --ratio 16:9 -n 3

# 利用可能な AI CLI を確認
pixa prompt --list-providers
```

### Web GUI

```bash
# サーバー起動
pixa-web

# ブラウザで http://localhost:3000 にアクセス
```

### Docker

```bash
docker compose up -d
# http://localhost:3000
```

## ビルド

### 前提条件

- Rust 1.75+
- CMake, NASM, pkg-config (mozjpeg ビルドに必要)

```bash
# Ubuntu/Debian
sudo apt install cmake nasm pkg-config libclang-dev

# macOS
brew install cmake nasm pkg-config

# ビルド
cargo build --release
```

### バイナリの場所

```
target/release/pixa      # CLI
target/release/pixa-web  # Web サーバー
```

## プロジェクト構成

```
pixa/
├── Cargo.toml              # ワークスペース
├── Dockerfile
├── docker-compose.yml
├── assets/                 # 埋め込みアルファマップ
│   ├── watermark_48x48.png
│   └── watermark_96x96.png
└── crates/
    ├── core/               # コアライブラリ
    │   └── src/
    │       ├── lib.rs
    │       ├── watermark.rs   # Reverse Alpha Blending
    │       ├── compress.rs    # 画像圧縮
    │       ├── convert.rs     # 形式変換
    │       ├── info.rs        # 情報表示
    │       └── prompt.rs      # プロンプト生成 (claude/gemini CLI 連携)
    ├── cli/                # CLI バイナリ
    │   └── src/main.rs
    └── web/                # Web サーバー + SPA
        ├── src/main.rs     # axum API サーバー
        └── static/
            └── index.html  # SPA フロントエンド
```

## API エンドポイント

| メソッド | パス                          | 説明                         |
| -------- | ----------------------------- | ---------------------------- |
| POST     | `/api/info`                   | 画像情報を JSON で返却       |
| POST     | `/api/detect-watermark`       | Watermark 検出               |
| POST     | `/api/remove-watermark`       | Watermark 削除（画像を返却） |
| POST     | `/api/compress?quality=80`    | 画像圧縮                     |
| POST     | `/api/convert?format=webp`    | 形式変換                     |
| GET      | `/api/prompt/providers`       | 利用可能な AI CLI 一覧       |
| POST     | `/api/prompt?description=...` | プロンプト生成               |
| GET      | `/api/health`                 | ヘルスチェック               |

すべての POST エンドポイントは `multipart/form-data` で `file` フィールドに画像を送信。

## Watermark 削除のしくみ

Gemini は以下の式で可視透かしを適用します:

```
watermarked = α × logo + (1 - α) × original
```

これを逆算して元のピクセル値を復元します:

```
original = (watermarked - α × logo) / (1 - α)
```

事前にキャリブレーションされた 48×48 / 96×96 のアルファマップを使用するため、AI 推論は不要で高速に動作します。

## ライセンス

MIT
