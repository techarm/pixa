# pixa 🖼️

Rust 製の高速画像処理 CLI ツールキット。

## 機能

| 機能               | 説明                                                               |
| ------------------ | ------------------------------------------------------------------ |
| **Watermark 削除** | Gemini AI の透かしを Reverse Alpha Blending で数学的に除去         |
| **画像軽量化**     | MozJPEG / OxiPNG / WebP による高品質圧縮                           |
| **形式変換**       | JPEG ↔ PNG ↔ WebP ↔ BMP ↔ GIF ↔ TIFF                               |
| **情報表示**       | 寸法・カラー・EXIF・SHA-256 等を一覧                               |
| **プロンプト生成** | ローカル AI CLI (claude/gemini) で Nanobanana 向けプロンプトを生成 |
| **Favicon 生成**   | 既存画像からブラウザ向けアイコンセットを生成                       |
| **背景除去**       | HSV ベースの緑背景除去 + 自動トリミング                            |

> Watermark 削除アルゴリズムは [GeminiWatermarkTool](https://github.com/allenk/GeminiWatermarkTool) by Allen Kuo (MIT License) を参考にしています。

## クイックスタート

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
├── assets/                 # 埋め込みアルファマップ
│   ├── watermark_48x48.png
│   └── watermark_96x96.png
└── src/
    ├── main.rs             # CLI エントリポイント
    ├── lib.rs
    ├── watermark.rs        # Reverse Alpha Blending
    ├── compress.rs
    ├── convert.rs
    ├── info.rs
    ├── prompt.rs
    ├── remove_bg.rs
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
