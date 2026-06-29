# Image Crate

> 图像处理能力 — 独立于 OCR 的通用图像处理层

## 核心原则

1. **Image 是独立 Capability，不是 OCR 附属**
2. **Core 永远不依赖 image crate，只认识 SnipperImage**
3. **Image 不知道 Tensor，Tensor 转换属于 Inference**

## 模块

| 模块 | 文件 | 说明 |
|---|---|---|
| `image` | image.rs | SnipperImage — 平台无关图像类型 |
| `view` | view.rs | ImageView — 零拷贝区域视图 |
| `operations` | operations.rs | resize/crop/letterbox/normalize/pad |
| `color` | color.rs | PixelFormat (Gray/Rgb/Rgba/Bgr/Bgra) |
| `decode` | decode.rs | 文件/内存解码 + PNG 编码 |

## 关键类型

### SnipperImage
```rust
pub struct SnipperImage {
    width: u32, height: u32,
    format: PixelFormat,
    pixels: Vec<u8>,
}
// 方法: width(), height(), format(), pixels(), get_pixel(), ...
```

### ImageView（零拷贝）
```rust
pub struct ImageView<'a> {
    image: &'a SnipperImage,
    rect: Rect,
}
// 避免 Detection → Recognition 过程中的像素复制
```

## Operations

| 函数 | 说明 |
|---|---|
| `resize(image, w, h)` | 最近邻缩放 |
| `resize_to_fit(image, max_side)` | 等比缩放到 max_side |
| `letterbox(image, target)` | YOLO letterbox + 返回 scale/pad |
| `normalize(image, mean, std)` | 归一化到 CHW f32 |
| `crop(image, rect)` | 裁剪区域 |
| `bgr_to_rgb / rgb_to_bgr` | 通道转换 |
| `pad_to_stride(image, stride)` | 填充到 stride 倍数 |

## 测试

14 项测试覆盖：创建、像素访问、视图提取、缩放、letterbox、归一化、裁剪、通道转换、填充。

## 依赖关系

```
Image
↑ 依赖 AST（Rect）
↓ 被 Inference, Pipeline, Export 依赖
```
