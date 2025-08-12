use reth_bsc::consensus::parlia::slash_pool;
use alloy_primitives::Address;

fn addr(n: u8) -> Address {
    Address::repeat_byte(n)
}

#[test]
fn slash_pool_deduplicates_and_drains() {
    // ensure pool starts empty
    assert!(slash_pool::drain().is_empty());

    // report same validator twice plus another one
    let v1 = addr(0x01);
    let v2 = addr(0x02);
    slash_pool::report(v1);
    slash_pool::report(v1); // duplicate
    slash_pool::report(v2);

    let mut drained = slash_pool::drain();
    drained.sort();
    assert_eq!(drained, vec![v1, v2]);

    // subsequent drain should be empty
    assert!(slash_pool::drain().is_empty());
} 