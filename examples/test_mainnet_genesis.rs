use alloy_primitives::hex;
use eyre::Result;

fn main() -> Result<()> {
    // BSC mainnet genesis extraData
    let mainnet_extra_data = "0x00000000000000000000000000000000000000000000000000000000000000002a7cdd959bfe8d9487b2a43b33565295a698f7e26488aa4d1955ee33403f8ccb1d4de5fb97c7ade29ef9f4360c606c7ab4db26b016007d3ad0ab86a0ee01c3b1283aa067c58eab4709f85e99d46de5fe685b1ded8013785d6623cc18d214320b6bb6475978f3adfc719c99674c072166708589033e2d9afec2be4ec20253b8642161bc3f444f53679c1f3d472f7be8361c80a4c1e7e9aaf001d0877f1cfde218ce2fd7544e0b2cc94692d4a704debef7bcb61328b8f7166496996a7da21cf1f1b04d9b3e26a3d0772d4c407bbe49438ed859fe965b140dcf1aab71a96bbad7cf34b5fa511d8e963dbba288b1960e75d64430b3230294d12c6ab2aac5c2cd68e80b16b581ea0a6e3c511bbd10f4519ece37dc24887e11b55d7ae2f5b9e386cd1b50a4550696d957cb4900f03a82012708dafc9e1b880fd083b32182b869be8e0922b81f8e175ffde54d797fe11eb03f9e3bf75f1d68bf0b8b6fb4e317a0f9d6f03eaf8ce6675bc60d8c4d90829ce8f72d0163c1d5cf348a862d55063035e7a025f4da968de7e4d7e4004197917f4070f1d6caa02bbebaebb5d7e581e4b66559e635f805ff0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    // Parse the hex data
    let data = hex::decode(&mainnet_extra_data[2..]).expect("Invalid hex");
    
    println!("🔍 BSC Mainnet Genesis ExtraData Analysis:");
    println!("   Total length: {} bytes", data.len());

    // First 32 bytes are vanity
    let vanity = &data[0..32];
    println!("   Vanity (32 bytes): {}", hex::encode(vanity));

    // Last 65 bytes are seal 
    let seal_start = data.len() - 65;
    let seal = &data[seal_start..];
    println!("   Seal (65 bytes): {}", hex::encode(seal));

    // Validator data is between vanity and seal
    let validator_data = &data[32..seal_start];
    println!("   Validator data length: {} bytes", validator_data.len());
    
    // Each validator address is 20 bytes
    let num_validators = validator_data.len() / 20;
    println!("   Number of validators: {}", num_validators);

    if validator_data.len() % 20 == 0 {
        println!("\n✅ Validator addresses:");
        for i in 0..num_validators {
            let start = i * 20;
            let end = start + 20;
            let addr = &validator_data[start..end];
            println!("      {}. 0x{}", i + 1, hex::encode(addr));
        }
    } else {
        println!("❌ Validator data length is not a multiple of 20 bytes");
    }

    Ok(())
}