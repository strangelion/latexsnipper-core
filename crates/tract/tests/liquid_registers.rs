use liquid_core::runtime::Registers;

#[test]
fn registers_preserve_mutations_and_isolate_types_and_instances() {
    let first = Registers::default();
    let second = Registers::default();
    *first.get_mut::<u32>() = 7;
    first.get_mut::<String>().push_str("kernel");
    for _ in 0..64 {
        *first.get_mut::<u32>() += 1;
    }
    assert_eq!(*first.get_mut::<u32>(), 71);
    assert_eq!(*first.get_mut::<u64>(), 0);
    assert_eq!(first.get_mut::<String>().as_str(), "kernel");
    assert_eq!(*second.get_mut::<u32>(), 0);
    assert!(second.get_mut::<String>().is_empty());
}

#[test]
fn conflicting_borrows_still_fail_without_losing_stored_state() {
    let registers = Registers::default();
    let mut held = registers.get_mut::<u32>();
    *held = 9;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(registers.get_mut::<String>());
    }));
    assert!(result.is_err());
    drop(held);
    assert_eq!(*registers.get_mut::<u32>(), 9);
    registers.get_mut::<String>().push_str("recovered");
    assert_eq!(registers.get_mut::<String>().as_str(), "recovered");
}
