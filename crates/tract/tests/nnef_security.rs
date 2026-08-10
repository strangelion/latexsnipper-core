use tract_nnef::tensors::read_tensor;

fn write_u32(header: &mut [u8], offset: usize, value: u32) {
    header[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn rejects_nnef_tensor_shape_products_that_overflow_usize() {
    let mut header = vec![0_u8; 128];
    header[0..4].copy_from_slice(&[0x4e, 0xef, 1, 0]);
    write_u32(&mut header, 8, 3); // rank
    write_u32(&mut header, 12, u32::MAX);
    write_u32(&mut header, 16, u32::MAX);
    write_u32(&mut header, 20, 3);
    write_u32(&mut header, 44, 32); // bits per item

    let error = read_tensor(header.as_slice()).expect_err("overflowing NNEF shape must fail");
    assert!(error.to_string().contains("overflows usize"));
}
