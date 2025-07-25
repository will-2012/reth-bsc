#![allow(unused)]

use crate::hardforks::bsc::BscHardfork;
use cfg_if::cfg_if;
use once_cell::{race::OnceBox, sync::Lazy};
use revm::{
    context::Cfg,
    context_interface::ContextTr,
    handler::{EthPrecompiles, PrecompileProvider},
    interpreter::{InputsImpl, InterpreterResult},
    precompile::{bls12_381, kzg_point_evaluation, modexp, secp256r1, Precompiles},
    primitives::{hardfork::SpecId, Address},
};
use std::boxed::Box;

mod bls;
mod cometbft;
mod double_sign;
mod error;
mod iavl;
mod tendermint;
mod tm_secp256k1;

// BSC precompile provider
#[derive(Debug, Clone)]
pub struct BscPrecompiles {
    /// Inner precompile provider is same as Ethereums.
    inner: EthPrecompiles,
}

impl BscPrecompiles {
    /// Create a new precompile provider with the given bsc spec.
    #[inline]
    // todo: check if we need to add cancun precompiles
    pub fn new(spec: BscHardfork) -> Self {
        tracing::info!("spec: {:?}", spec);
        let precompiles = if spec >= BscHardfork::Haber {
            haber()
        } else if spec >= BscHardfork::Feynman {
            feynman()
        } else if spec >= BscHardfork::Hertz {
            hertz()
        } else if spec >= BscHardfork::Plato {
            plato()
        } else if spec >= BscHardfork::Luban {
            luban()
        } else if spec >= BscHardfork::Planck {
            planck()
        } else if spec >= BscHardfork::Moran {
            moran()
        } else if spec >= BscHardfork::Nano {
            nano()
        } else {
            istanbul()
        };

        Self { inner: EthPrecompiles { precompiles, spec: spec.into() } }
    }

    #[inline]
    pub fn precompiles(&self) -> &'static Precompiles {
        self.inner.precompiles
    }
}

/// Returns precompiles for Istanbul spec.
pub fn istanbul() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = Precompiles::istanbul().clone();
        precompiles.extend([tendermint::TENDERMINT_HEADER_VALIDATION, iavl::IAVL_PROOF_VALIDATION]);
        Box::new(precompiles)
    })
}

/// Returns precompiles for Nano spec.
pub fn nano() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = istanbul().clone();
        precompiles.extend([
            tendermint::TENDERMINT_HEADER_VALIDATION_NANO,
            iavl::IAVL_PROOF_VALIDATION_NANO,
        ]);
        Box::new(precompiles)
    })
}

/// Returns precompiles for Moran sepc.
pub fn moran() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = istanbul().clone();
        precompiles
            .extend([tendermint::TENDERMINT_HEADER_VALIDATION, iavl::IAVL_PROOF_VALIDATION_MORAN]);

        Box::new(precompiles)
    })
}

/// Returns precompiles for Planck sepc.
pub fn planck() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = istanbul().clone();
        precompiles
            .extend([tendermint::TENDERMINT_HEADER_VALIDATION, iavl::IAVL_PROOF_VALIDATION_PLANCK]);

        Box::new(precompiles)
    })
}

/// Returns precompiles for Luban sepc.
pub fn luban() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = planck().clone();
        precompiles.extend([
            bls::BLS_SIGNATURE_VALIDATION,
            cometbft::COMETBFT_LIGHT_BLOCK_VALIDATION_BEFORE_HERTZ,
        ]);

        Box::new(precompiles)
    })
}

/// Returns precompiles for Plato sepc.
pub fn plato() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = luban().clone();
        precompiles.extend([iavl::IAVL_PROOF_VALIDATION_PLATO]);

        Box::new(precompiles)
    })
}

/// Returns precompiles for Hertz sepc.
pub fn hertz() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = plato().clone();
        precompiles.extend([cometbft::COMETBFT_LIGHT_BLOCK_VALIDATION, modexp::BERLIN]);

        Box::new(precompiles)
    })
}

/// Returns precompiles for Feynman sepc.
pub fn feynman() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = hertz().clone();
        precompiles.extend([
            double_sign::DOUBLE_SIGN_EVIDENCE_VALIDATION,
            tm_secp256k1::TM_SECP256K1_SIGNATURE_RECOVER,
        ]);
        Box::new(precompiles)
    })
}

/// Returns precompiles for Haber spec.
pub fn haber() -> &'static Precompiles {
    static INSTANCE: OnceBox<Precompiles> = OnceBox::new();
    INSTANCE.get_or_init(|| {
        let mut precompiles = feynman().clone();
        precompiles.extend([kzg_point_evaluation::POINT_EVALUATION, secp256r1::P256VERIFY]);

        Box::new(precompiles)
    })
}

impl<CTX> PrecompileProvider<CTX> for BscPrecompiles
where
    CTX: ContextTr<Cfg: Cfg<Spec = BscHardfork>>,
{
    type Output = InterpreterResult;

    #[inline]
    fn set_spec(&mut self, spec: <CTX::Cfg as Cfg>::Spec) -> bool {
        *self = Self::new(spec);
        true
    }

    #[inline]
    fn run(
        &mut self,
        context: &mut CTX,
        address: &Address,
        inputs: &InputsImpl,
        is_static: bool,
        gas_limit: u64,
    ) -> Result<Option<Self::Output>, String> {
        self.inner.run(context, address, inputs, is_static, gas_limit)
    }

    #[inline]
    fn warm_addresses(&self) -> Box<impl Iterator<Item = Address>> {
        self.inner.warm_addresses()
    }

    #[inline]
    fn contains(&self, address: &Address) -> bool {
        self.inner.contains(address)
    }
}

impl Default for BscPrecompiles {
    fn default() -> Self {
        Self::new(BscHardfork::default())
    }
}
