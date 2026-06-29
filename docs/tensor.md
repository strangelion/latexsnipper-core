# Tensor Crate

> 推理 I/O 的多维数组 — Image / Inference / Runtime 共享

## 核心原则

1. **Tensor 是 Runtime 和 Inference 之间的数据契约**
2. **不依赖 Image Crate，Image → Tensor 转换属于 Inference**
3. **支持序列化，方便 FFI/WASM 边界传递**

## 数据模型

```
Tensor
├── name: String          // 张量名称（如 "input", "output"）
├── shape: Vec<usize>     // 维度（如 [1, 3, 768, 768]）
└── data: TensorData      // 类型化数据
    ├── Float32(Vec<f32>) // ONNX 推理常用
    ├── Int64(Vec<i64>)   // Token ID 序列
    ├── Int32(Vec<i32>)   // 辅助索引
    └── UInt8(Vec<u8>)    // 量化模型输入
```

## 关键类型

### Tensor

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tensor {
    name: String,
    shape: Vec<usize>,
    data: TensorData,
}
```

### TensorData

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TensorData {
    Float32(Vec<f32>),
    Int64(Vec<i64>),
    Int32(Vec<i32>),
    UInt8(Vec<u8>),
}
```

## 构造方法

| 方法 | 说明 |
|------|------|
| `Tensor::float32(name, shape, data)` | 创建 Float32 张量 |
| `Tensor::int64(name, shape, data)` | 创建 Int64 张量 |

## 访问方法

| 方法 | 返回 | 说明 |
|------|------|------|
| `name()` | `&str` | 张量名称 |
| `shape()` | `&[usize]` | 维度信息 |
| `data()` | `&TensorData` | 原始数据 |
| `as_f32_slice()` | `Option<&[f32]>` | Float32 视图 |
| `as_i64_slice()` | `Option<&[i64]>` | Int64 视图 |
| `len()` | `usize` | 元素总数 |
| `is_empty()` | `bool` | 是否为空 |

## 使用示例

```rust
// 创建输入张量（图像归一化后）
let input = Tensor::float32(
    "input",
    vec![1, 3, 768, 768],
    normalized_pixels,  // Vec<f32>
);

// 创建输出张量（YOLO 检测结果）
let output = Tensor::float32(
    "output",
    vec![1, 84, 8400],
    detection_data,
);

// 传递给 Runtime
let results = session.run(&[input])?;
let output_tensor = &results[0];
if let Some(detections) = output_tensor.as_f32_slice() {
    // 处理检测结果
}
```

## 依赖关系

```
Tensor
↑ 不依赖 Image（转换属于 Inference 层）
↓ 被 Runtime, Inference 依赖
```

**关键约束：Image Crate 永远不直接操作 Tensor。**
`SnipperImage` → `Tensor` 的转换发生在 Inference 层，由 `normalize()` 等操作完成。
