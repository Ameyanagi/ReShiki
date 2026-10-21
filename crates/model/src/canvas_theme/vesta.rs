//! VESTA element colours (K. Momma and F. Izumi, https://jp-minerals.org/vesta/),
//! via pymatgen ElementColorSchemes.yaml (MIT):
//! https://github.com/materialsproject/pymatgen/blob/master/src/pymatgen/vis/ElementColorSchemes.yaml
//! The table covers H–Mt in `crate::editing::ELEMENTS` order; later elements retain neutral colors.

pub(crate) const SOURCE: &str = "VESTA element colours (K. Momma and F. Izumi, https://jp-minerals.org/vesta/), via pymatgen ElementColorSchemes.yaml";

const VESTA: [u32; 109] = [
    0xFFCCCC, // H
    0xFCE8CE, // He
    0x86DF73, // Li
    0x5ED77B, // Be
    0x1FA20F, // B
    0x4C4C4C, // C
    0xB0B9E6, // N
    0xFE0300, // O
    0xB0B9E6, // F
    0xFE37B5, // Ne
    0xF9DC3C, // Na
    0xFB7B15, // Mg
    0x81B2D6, // Al
    0x1B3BFA, // Si
    0xC09CC2, // P
    0xFFFA00, // S
    0x31FC02, // Cl
    0xCFFEC4, // Ar
    0xA121F6, // K
    0x5A96BD, // Ca
    0xB563AB, // Sc
    0x78CAFF, // Ti
    0xE51900, // V
    0x00009E, // Cr
    0xA7089D, // Mn
    0xB57100, // Fe
    0x0000AF, // Co
    0xB7BBBD, // Ni
    0x2247DC, // Cu
    0x8F8F81, // Zn
    0x9EE373, // Ga
    0x7E6EA6, // Ge
    0x74D057, // As
    0x9AEF0F, // Se
    0x7E3102, // Br
    0xFAC1F3, // Kr
    0x702EB0, // Rb
    0x00FF00, // Sr
    0x94FFFF, // Y
    0x00FF00, // Zr
    0x73C2C9, // Nb
    0x54B5B5, // Mo
    0x3B9E9E, // Tc
    0x248F8F, // Ru
    0x0A7D8C, // Rh
    0x006985, // Pd
    0xC0C0C0, // Ag
    0xFFD98F, // Cd
    0xA67573, // In
    0x9A8EB9, // Sn
    0x9E63B5, // Sb
    0xD47A00, // Te
    0x940094, // I
    0x429EB0, // Xe
    0x57178F, // Cs
    0x00C900, // Ba
    0x5AC449, // La
    0xFFFFC7, // Ce
    0xD9FFC7, // Pr
    0xC7FFC7, // Nd
    0xA3FFC7, // Pm
    0x8FFFC7, // Sm
    0x61FFC7, // Eu
    0x45FFC7, // Gd
    0x30FFC7, // Tb
    0x1FFFC7, // Dy
    0x00FF9C, // Ho
    0x00E675, // Er
    0x00D452, // Tm
    0x00BF38, // Yb
    0x00AB24, // Lu
    0x4DC2FF, // Hf
    0x4DA6FF, // Ta
    0x2194D6, // W
    0x267DAB, // Re
    0x266696, // Os
    0x175487, // Ir
    0xD0D0E0, // Pt
    0xFFD123, // Au
    0xB8B8D0, // Hg
    0xA6544D, // Tl
    0x575961, // Pb
    0x9E4FB5, // Bi
    0xAB5C00, // Po
    0x754F45, // At
    0x428296, // Rn
    0x420066, // Fr
    0x007D00, // Ra
    0x70ABFA, // Ac
    0x00BAFF, // Th
    0x00A1FF, // Pa
    0x008FFF, // U
    0x0080FF, // Np
    0x006BFF, // Pu
    0x545CF2, // Am
    0x785CE3, // Cm
    0x8A4FE3, // Bk
    0xA136D4, // Cf
    0xB31FD4, // Es
    0xB31FBA, // Fm
    0xB30DA6, // Md
    0xBD0D87, // No
    0xC70066, // Lr
    0xCC0059, // Rf
    0xD1004F, // Db
    0xD90045, // Sg
    0xE00038, // Bh
    0xE6002E, // Hs
    0xEB0026, // Mt
];

pub(crate) fn swatch(element: &str) -> Option<[u8; 3]> {
    super::jmol::lookup(&VESTA, element)
}
