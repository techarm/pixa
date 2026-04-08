# pixa 🖼️

Rust 製の高速画像処理 CLI ツールキット。

## 機能

| 機能               | 説明                                                       |
| ------------------ | ---------------------------------------------------------- |
| **Watermark 削除** | Gemini AI の透かしを Reverse Alpha Blending で数学的に除去 |
| **Watermark 検出** | 透かしの有無をスコア付きで判定                             |
| **画像軽量化**     | MozJPEG / OxiPNG / WebP による高品質圧縮                   |
| **形式変換**       | JPEG ↔ PNG ↔ WebP ↔ BMP ↔ GIF ↔ TIFF                       |
| **情報表示**       | 寸法・カラー・EXIF・SHA-256 等を一覧                       |
| **Favicon 生成**   | 画像からブラウザ向けアイコンセットを生成                   |

`remove-watermark` / `compress` / `convert` はファイルとディレクトリの両方を受け付けます。ディレクトリを処理する場合は `-r/--recursive` を付けてください。

> Watermark 削除アルゴリズムは [GeminiWatermarkTool](https://github.com/allenk/GeminiWatermarkTool) by Allen Kuo (MIT License) を参考にしています。

## クイックスタート

```bash
# Watermark 削除（単一）
pixa remove-watermark image.jpg -o clean.jpg

# Watermark 削除（ディレクトリ再帰）
pixa remove-watermark ./photos -r -o ./cleaned

# Watermark 検出
pixa detect image.jpg

# 画像圧縮（単一）
pixa compress input.png -o output.png -q 80

# 画像圧縮（ディレクトリ再帰）
pixa compress ./photos -r -o ./compressed -q 85

# 形式変換（単一）
pixa convert photo.png photo.webp

# 形式変換（ディレクトリ再帰）
pixa convert ./photos ./webp -r --format webp

# 画像情報
pixa info photo.jpg --json

# Favicon セット生成
pixa favicon logo.png -o ./favicon-output
```

## ビルド

### 前提条件

- Rust 1.87+
- CMake, NASM, pkg-config (mozjpeg ビルドに必要)

```bash
# Ubuntu/Debian
sudo apt install cmake nasm pkg-config libclang-dev

# macOS
brew install cmake nasm pkg-config

# ビルド
cargo build --release
```

バイナリは `target/release/pixa` に出力されます。

## プロジェクト構成

```
pixa/
├── Cargo.toml
├── assets/                       # 埋め込みアルファマップ
│   ├── watermark_48x48.png
│   └── watermark_96x96.png
└── src/
    ├── main.rs                   # CLI エントリポイント
    ├── lib.rs                    # ライブラリ API
    ├── watermark.rs              # Reverse Alpha Blending
    ├── compress.rs
    ├── convert.rs
    ├── info.rs
    ├── favicon.rs
    └── commands/                 # 各サブコマンド実装
        ├── mod.rs                # 共通ユーティリティ
        ├── remove_watermark.rs
        ├── detect.rs
        ├── compress.rs
        ├── convert.rs
        ├── info.rs
        └── favicon.rs
```

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
