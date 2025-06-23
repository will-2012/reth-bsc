use revm::primitives::hardfork::SpecId;
use std::str::FromStr;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(non_camel_case_types)]
#[allow(clippy::upper_case_acronyms)]
pub enum BscSpecId {
    FRONTIER = 0, // Frontier
    RAMANUJAN,    // Ramanujan
    NIELS,        // Niels
    MIRROR_SYNC,  // Mirror Sync
    BRUNO,        // Bruno
    EULER,        // Euler
    NANO,         // Nano
    MORAN,        // Moran
    GIBBS,        // Gibbs
    PLANCK,       // Planck
    LUBAN,        // Luban
    PLATO,        // Plato
    HERTZ,        // Hertz
    HERTZ_FIX,    // HertzFix
    KEPLER,       // Kepler
    FEYNMAN,      // Feynman
    FEYNMAN_FIX,  // FeynmanFix
    HABER,        // Haber
    HABER_FIX,    // HaberFix
    BOHR,         // Bohr
    PASCAL,       // Pascal
    #[default]
    LORENTZ, // Lorentz
}

impl BscSpecId {
    pub const fn is_enabled_in(self, other: BscSpecId) -> bool {
        other as u8 <= self as u8
    }

    /// Converts the [`BscSpecId`] into a [`SpecId`].
    pub const fn into_eth_spec(self) -> SpecId {
        match self {
            Self::FRONTIER |
            Self::RAMANUJAN |
            Self::NIELS |
            Self::MIRROR_SYNC |
            Self::BRUNO |
            Self::EULER |
            Self::GIBBS |
            Self::NANO |
            Self::MORAN |
            Self::PLANCK |
            Self::LUBAN |
            Self::PLATO => SpecId::MUIR_GLACIER,
            Self::HERTZ | Self::HERTZ_FIX => SpecId::LONDON,
            Self::KEPLER | Self::FEYNMAN | Self::FEYNMAN_FIX => SpecId::SHANGHAI,
            Self::HABER | Self::HABER_FIX | Self::BOHR | Self::PASCAL | Self::LORENTZ => {
                SpecId::CANCUN
            }
        }
    }
}

impl From<BscSpecId> for SpecId {
    fn from(spec: BscSpecId) -> Self {
        spec.into_eth_spec()
    }
}

/// String identifiers for BSC hardforks
pub mod name {
    pub const FRONTIER: &str = "Frontier";
    pub const RAMANUJAN: &str = "Ramanujan";
    pub const NIELS: &str = "Niels";
    pub const MIRROR_SYNC: &str = "MirrorSync";
    pub const BRUNO: &str = "Bruno";
    pub const EULER: &str = "Euler";
    pub const NANO: &str = "Nano";
    pub const MORAN: &str = "Moran";
    pub const GIBBS: &str = "Gibbs";
    pub const PLANCK: &str = "Planck";
    pub const LUBAN: &str = "Luban";
    pub const PLATO: &str = "Plato";
    pub const HERTZ: &str = "Hertz";
    pub const HERTZ_FIX: &str = "HertzFix";
    pub const KEPLER: &str = "Kepler";
    pub const FEYNMAN: &str = "Feynman";
    pub const FEYNMAN_FIX: &str = "FeynmanFix";
    pub const HABER: &str = "Haber";
    pub const HABER_FIX: &str = "HaberFix";
    pub const BOHR: &str = "Bohr";
    pub const PASCAL: &str = "Pascal";
    pub const LORENTZ: &str = "Lorentz";
}

impl FromStr for BscSpecId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            name::RAMANUJAN => Self::RAMANUJAN,
            name::NIELS => Self::NIELS,
            name::MIRROR_SYNC => Self::MIRROR_SYNC,
            name::BRUNO => Self::BRUNO,
            name::EULER => Self::EULER,
            name::NANO => Self::NANO,
            name::MORAN => Self::MORAN,
            name::GIBBS => Self::GIBBS,
            name::PLANCK => Self::PLANCK,
            name::LUBAN => Self::LUBAN,
            name::PLATO => Self::PLATO,
            name::HERTZ => Self::HERTZ,
            name::HERTZ_FIX => Self::HERTZ_FIX,
            name::KEPLER => Self::KEPLER,
            name::FEYNMAN => Self::FEYNMAN,
            name::FEYNMAN_FIX => Self::FEYNMAN_FIX,
            name::HABER => Self::HABER,
            name::HABER_FIX => Self::HABER_FIX,
            name::BOHR => Self::BOHR,
            _ => return Err(format!("Unknown BSC spec: {s}")),
        })
    }
}

impl From<BscSpecId> for &'static str {
    fn from(spec_id: BscSpecId) -> Self {
        match spec_id {
            BscSpecId::FRONTIER => name::FRONTIER,
            BscSpecId::RAMANUJAN => name::RAMANUJAN,
            BscSpecId::NIELS => name::NIELS,
            BscSpecId::MIRROR_SYNC => name::MIRROR_SYNC,
            BscSpecId::BRUNO => name::BRUNO,
            BscSpecId::EULER => name::EULER,
            BscSpecId::NANO => name::NANO,
            BscSpecId::MORAN => name::MORAN,
            BscSpecId::GIBBS => name::GIBBS,
            BscSpecId::PLANCK => name::PLANCK,
            BscSpecId::LUBAN => name::LUBAN,
            BscSpecId::PLATO => name::PLATO,
            BscSpecId::HERTZ => name::HERTZ,
            BscSpecId::HERTZ_FIX => name::HERTZ_FIX,
            BscSpecId::KEPLER => name::KEPLER,
            BscSpecId::FEYNMAN => name::FEYNMAN,
            BscSpecId::FEYNMAN_FIX => name::FEYNMAN_FIX,
            BscSpecId::HABER => name::HABER,
            BscSpecId::HABER_FIX => name::HABER_FIX,
            BscSpecId::BOHR => name::BOHR,
            BscSpecId::PASCAL => name::PASCAL,
            BscSpecId::LORENTZ => name::LORENTZ,
        }
    }
}
