//! Frozen GNU 31.1 data; generated from the captured bytecode/object graph.
//! Threading: immutable scalar descriptors only; no Lisp state or caches.

#[derive(Clone, Copy, Debug)]
pub(super) enum ObjectSpec {
    Float(u64),
    Vector([usize; 7]),
}
#[derive(Clone, Copy, Debug)]
pub(super) enum ConstantSpec {
    Object(usize),
    Integer(i64),
    Symbol(&'static str),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct FunctionSpec {
    pub name: &'static str,
    pub descriptor: i64,
    pub max_stack: u16,
    pub bytes: &'static [u8],
    pub constants: &'static [ConstantSpec],
}

pub(super) const OBJECTS: [ObjectSpec; 42] = [
    ObjectSpec::Float(0x4076d3d70a3d70a4),     // original object 0
    ObjectSpec::Float(0x4043bd3cc9be45de),     // original object 1
    ObjectSpec::Vector([3, 3, 3, 3, 3, 3, 1]), // original object 2
    ObjectSpec::Float(0x0000000000000000),     // original object 3
    ObjectSpec::Vector([5, 6, 7, 8, 9, 10, 11]), // original object 4
    ObjectSpec::Float(0x40135da0343cd92c),     // original object 5
    ObjectSpec::Float(0xbff290abc01fdb7c),     // original object 6
    ObjectSpec::Float(0xbfba86f96c25ebf0),     // original object 7
    ObjectSpec::Float(0x3fe367069b93ccbc),     // original object 8
    ObjectSpec::Float(0x40067ef2f57d949b),     // original object 9
    ObjectSpec::Float(0xbf99d2d79a5a0715),     // original object 10
    ObjectSpec::Float(0x3fa34c95d9ab33d8),     // original object 11
    ObjectSpec::Vector([13, 14, 15, 16, 17, 18, 19]), // original object 12
    ObjectSpec::Float(0x4020afcdc332ca67),     // original object 13
    ObjectSpec::Float(0x40107fcb31de01b0),     // original object 14
    ObjectSpec::Float(0xbfd9d353e1eb467c),     // original object 15
    ObjectSpec::Float(0xbff02c21b8879442),     // original object 16
    ObjectSpec::Float(0x3ffd35e9bf1f8f13),     // original object 17
    ObjectSpec::Float(0x3f813c485f1123b4),     // original object 18
    ObjectSpec::Float(0x3f871d490d07c637),     // original object 19
    ObjectSpec::Vector([21, 22, 23, 24, 25, 26, 27]), // original object 20
    ObjectSpec::Float(0x4029c9eacea7d9cf),     // original object 21
    ObjectSpec::Float(0xc02e38e8d626667e),     // original object 22
    ObjectSpec::Float(0xbfcc9557be257da0),     // original object 23
    ObjectSpec::Float(0x3ff1531ca9911bef),     // original object 24
    ObjectSpec::Float(0x3febcc7f3e54bbc5),     // original object 25
    ObjectSpec::Float(0xbf862f6bfaf23e7c),     // original object 26
    ObjectSpec::Float(0x3f5c3dd29cf41eb3),     // original object 27
    ObjectSpec::Vector([29, 30, 31, 32, 33, 34, 35]), // original object 28
    ObjectSpec::Float(0x402ec267a905572a),     // original object 29
    ObjectSpec::Float(0xc039eb5833c8a220),     // original object 30
    ObjectSpec::Float(0x3fc6f1f393abe540),     // original object 31
    ObjectSpec::Float(0x3fef54b61659bc4a),     // original object 32
    ObjectSpec::Float(0x3fe307c631c4fba3),     // original object 33
    ObjectSpec::Float(0xbfa1cb88587665f6),     // original object 34
    ObjectSpec::Float(0x3f60a8f3531799ac),     // original object 35
    ObjectSpec::Float(0x3ff0000000000000),     // original object 36
    ObjectSpec::Float(0x3ff000001ad7f29b),     // original object 37
    ObjectSpec::Float(0x3fe0000000000000),     // original object 38
    ObjectSpec::Float(0x0000000000000000),     // original object 39
    ObjectSpec::Float(0x0000000000000000),     // original object 40
    ObjectSpec::Float(0x3fe0000000000000),     // original object 41
];

pub(super) const GLOBALS: [(&str, ConstantSpec); 7] = [
    ("elb-days-per-year", ConstantSpec::Object(0)),
    ("elb-solar-mass", ConstantSpec::Object(1)),
    ("elb-sun", ConstantSpec::Object(2)),
    ("elb-jupiter", ConstantSpec::Object(4)),
    ("elb-saturn", ConstantSpec::Object(12)),
    ("elb-uranus", ConstantSpec::Object(20)),
    ("elb-neptune", ConstantSpec::Object(28)),
];

pub(super) const FUNCTIONS: [FunctionSpec; 6] = [
    FunctionSpec {
        name: "t34-o36-allocation-float",
        descriptor: 257,
        max_stack: 5,
        bytes: &[
            192, 193, 137, 3, 87, 131, 19, 0, 1, 194, 95, 195, 92, 178, 2, 84, 130, 2, 0, 1, 135,
        ],
        constants: &[
            ConstantSpec::Object(36),
            ConstantSpec::Integer(0),
            ConstantSpec::Object(37),
            ConstantSpec::Object(38),
        ],
    },
    FunctionSpec {
        name: "t34-o36-allocation-setup",
        descriptor: 0,
        max_stack: 6,
        bytes: &[
            197, 8, 33, 197, 9, 33, 197, 10, 33, 197, 11, 33, 197, 12, 33, 175, 5, 135,
        ],
        constants: &[
            ConstantSpec::Symbol("elb-sun"),
            ConstantSpec::Symbol("elb-jupiter"),
            ConstantSpec::Symbol("elb-saturn"),
            ConstantSpec::Symbol("elb-uranus"),
            ConstantSpec::Symbol("elb-neptune"),
            ConstantSpec::Symbol("copy-sequence"),
        ],
    },
    FunctionSpec {
        name: "elb-advance",
        descriptor: 514,
        max_stack: 12,
        bytes: &[
            1, 192, 137, 2, 58, 131, 46, 0, 2, 178, 2, 1, 137, 65, 178, 3, 178, 1, 162, 1, 137,
            131, 38, 0, 137, 64, 193, 3, 2, 6, 8, 35, 182, 2, 65, 130, 20, 0, 136, 2, 65, 178, 3,
            130, 3, 0, 182, 3, 1, 137, 131, 99, 0, 137, 64, 137, 194, 2, 194, 72, 5, 4, 195, 72,
            95, 92, 73, 136, 137, 196, 2, 196, 72, 5, 4, 197, 72, 95, 92, 73, 136, 137, 198, 2,
            198, 72, 5, 4, 199, 72, 95, 92, 73, 182, 2, 65, 130, 49, 0, 192, 135,
        ],
        constants: &[
            ConstantSpec::Symbol("nil"),
            ConstantSpec::Symbol("elb-applyforces"),
            ConstantSpec::Integer(0),
            ConstantSpec::Integer(3),
            ConstantSpec::Integer(1),
            ConstantSpec::Integer(4),
            ConstantSpec::Integer(2),
            ConstantSpec::Integer(5),
        ],
    },
    FunctionSpec {
        name: "elb-applyforces",
        descriptor: 771,
        max_stack: 18,
        bytes: &[
            2, 192, 72, 2, 192, 72, 90, 3, 193, 72, 3, 193, 72, 90, 4, 194, 72, 4, 194, 72, 90,
            195, 3, 137, 95, 3, 137, 95, 92, 2, 137, 95, 92, 33, 4, 1, 137, 95, 2, 95, 165, 4, 1,
            95, 4, 2, 95, 4, 3, 95, 6, 10, 196, 6, 12, 196, 72, 5, 6, 13, 197, 72, 95, 90, 73, 136,
            6, 10, 198, 6, 12, 198, 72, 4, 6, 13, 197, 72, 95, 90, 73, 136, 6, 10, 199, 6, 12, 199,
            72, 3, 6, 13, 197, 72, 95, 90, 73, 136, 6, 9, 196, 6, 11, 196, 72, 5, 6, 14, 197, 72,
            95, 92, 73, 136, 6, 9, 198, 6, 11, 198, 72, 4, 6, 14, 197, 72, 95, 92, 73, 136, 6, 9,
            199, 6, 11, 199, 72, 3, 6, 14, 197, 72, 95, 92, 73, 200, 135,
        ],
        constants: &[
            ConstantSpec::Integer(0),
            ConstantSpec::Integer(1),
            ConstantSpec::Integer(2),
            ConstantSpec::Symbol("sqrt"),
            ConstantSpec::Integer(3),
            ConstantSpec::Integer(6),
            ConstantSpec::Integer(4),
            ConstantSpec::Integer(5),
            ConstantSpec::Symbol("nil"),
        ],
    },
    FunctionSpec {
        name: "elb-offset-momentum",
        descriptor: 257,
        max_stack: 11,
        bytes: &[
            193, 137, 137, 3, 137, 131, 48, 0, 137, 64, 4, 1, 194, 72, 2, 195, 72, 95, 92, 178, 5,
            3, 1, 196, 72, 2, 195, 72, 95, 92, 178, 4, 2, 1, 197, 72, 2, 195, 72, 95, 92, 178, 3,
            136, 65, 130, 4, 0, 136, 3, 64, 137, 194, 5, 91, 8, 165, 73, 182, 2, 3, 64, 137, 196,
            4, 91, 8, 165, 73, 182, 2, 3, 64, 137, 197, 3, 91, 8, 165, 73, 198, 135,
        ],
        constants: &[
            ConstantSpec::Symbol("elb-solar-mass"),
            ConstantSpec::Object(39),
            ConstantSpec::Integer(3),
            ConstantSpec::Integer(6),
            ConstantSpec::Integer(4),
            ConstantSpec::Integer(5),
            ConstantSpec::Symbol("nil"),
        ],
    },
    FunctionSpec {
        name: "elb-energy",
        descriptor: 257,
        max_stack: 15,
        bytes: &[
            192, 1, 193, 137, 2, 58, 131, 124, 0, 2, 178, 2, 1, 137, 65, 178, 3, 178, 1, 162, 3, 1,
            194, 72, 195, 95, 2, 196, 72, 3, 196, 72, 95, 3, 197, 72, 4, 197, 72, 95, 92, 3, 198,
            72, 4, 198, 72, 95, 92, 95, 92, 178, 4, 1, 137, 131, 116, 0, 137, 64, 2, 199, 72, 1,
            199, 72, 90, 3, 200, 72, 2, 200, 72, 90, 4, 201, 72, 3, 201, 72, 90, 202, 3, 137, 95,
            3, 137, 95, 92, 2, 137, 95, 92, 33, 6, 9, 6, 7, 194, 72, 6, 6, 194, 72, 95, 2, 165, 90,
            178, 10, 182, 5, 65, 130, 54, 0, 136, 2, 65, 178, 3, 130, 4, 0, 182, 3, 135,
        ],
        constants: &[
            ConstantSpec::Object(40),
            ConstantSpec::Symbol("nil"),
            ConstantSpec::Integer(6),
            ConstantSpec::Object(41),
            ConstantSpec::Integer(3),
            ConstantSpec::Integer(4),
            ConstantSpec::Integer(5),
            ConstantSpec::Integer(0),
            ConstantSpec::Integer(1),
            ConstantSpec::Integer(2),
            ConstantSpec::Symbol("sqrt"),
        ],
    },
];

pub(super) const FLOAT_ANSWERS: [(i64, u64); 4] = [
    (0, 0x3ff0000000000000),
    (1, 0x3ff800001ad7f29b),
    (128, 0x40504006de427e31),
    (1024, 0x40800835d8a19538),
];

pub(super) const NBODY_ANSWERS: [(usize, u64, [u64; 35]); 3] = [
    (
        0,
        0xbfc5a441459acc23,
        [
            0x0000000000000000,
            0x0000000000000000,
            0x0000000000000000,
            0xbf3967e9a7e0d6f3,
            0xbf6ad4ecfe5089fb,
            0x3ef919331f0b8a71,
            0x4043bd3cc9be45de,
            0x40135da0343cd92c,
            0xbff290abc01fdb7c,
            0xbfba86f96c25ebf0,
            0x3fe367069b93ccbc,
            0x40067ef2f57d949b,
            0xbf99d2d79a5a0715,
            0x3fa34c95d9ab33d8,
            0x4020afcdc332ca67,
            0x40107fcb31de01b0,
            0xbfd9d353e1eb467c,
            0xbff02c21b8879442,
            0x3ffd35e9bf1f8f13,
            0x3f813c485f1123b4,
            0x3f871d490d07c637,
            0x4029c9eacea7d9cf,
            0xc02e38e8d626667e,
            0xbfcc9557be257da0,
            0x3ff1531ca9911bef,
            0x3febcc7f3e54bbc5,
            0xbf862f6bfaf23e7c,
            0x3f5c3dd29cf41eb3,
            0x402ec267a905572a,
            0xc039eb5833c8a220,
            0x3fc6f1f393abe540,
            0x3fef54b61659bc4a,
            0x3fe307c631c4fba3,
            0xbfa1cb88587665f6,
            0x3f60a8f3531799ac,
        ],
    ),
    (
        100,
        0xbfc5a37493e5edab,
        [
            0x3f3c95e4554e5e63,
            0xbf6ad8ed3e4b2442,
            0x3ed11b5942c6fad9,
            0x3f5444e22f5d69e1,
            0xbf69a06c5c52673e,
            0xbef0a6cd70daf365,
            0x4043bd3cc9be45de,
            0x4012962548b28c2e,
            0x3ffafac6a633b78a,
            0xbfbc6d9f9cd033ef,
            0xbfef375afb3c1d6c,
            0x4005d0f28ff1fb60,
            0x3f85990fecaa4aff,
            0x3fa34c95d9ab33d8,
            0x401c88df36c0b922,
            0x40175490c06c5634,
            0xbfd8a61c8253427c,
            0xbff6590dabb5b72a,
            0x3ff93b0b63b1d664,
            0x3f9cc90906efefbb,
            0x3f871d490d07c637,
            0x402be3137aad8913,
            0xc02c68d76a987090,
            0xbfcde56cc535f977,
            0x3ff03edda7cf8536,
            0x3fee28ca9faf66c8,
            0xbf83d551286bd669,
            0x3f5c3dd29cf41eb3,
            0x403058eb04a9ae20,
            0xc0394e4bb3dc92bc,
            0x3fc27b0b2cc0d1e7,
            0x3fee99038a6d6b7f,
            0x3fe4372a412ea8d7,
            0xbfa1ea52f4de092d,
            0x3f60a8f3531799ac,
        ],
    ),
    (
        400,
        0xbfc5a2350011cdfc,
        [
            0x3f812dc124d502be,
            0xbf7dc5b758eb81f6,
            0xbf26b9233525470b,
            0x3f67f0a26103ef6d,
            0x3f4b463a8964e576,
            0xbf1472eee2ecccfd,
            0x4043bd3cc9be45de,
            0xc00288247955fb61,
            0x4012c9d0df09da34,
            0x3fa08c38258c9428,
            0xc0041889f13dc67b,
            0xbff1354d8adc6063,
            0x3faf10446f4fd29a,
            0x3fa34c95d9ab33d8,
            0x3ffa9d6acc863645,
            0x4021c7fd5bcf6eff,
            0xbfcc3d82f024676f,
            0xc000e3b676a29955,
            0x3fd7fbc63d40eb8d,
            0x3fb3d8d55deb3927,
            0x3f871d490d07c637,
            0x4030a778fc1741fb,
            0xc02628d8a77b6615,
            0xbfd07557c5ee46a1,
            0x3fe9299c97e392c7,
            0x3ff21c914e560809,
            0xbf789e00a98e5e58,
            0x3f5c3dd29cf41eb3,
            0x40331a594c00f259,
            0xc0373fdd0a443c3c,
            0x3fa3cf230ed16607,
            0x3fec2183f6a29427,
            0x3fe798d7a98ba439,
            0xbfa21ed83f56f4a6,
            0x3f60a8f3531799ac,
        ],
    ),
];
