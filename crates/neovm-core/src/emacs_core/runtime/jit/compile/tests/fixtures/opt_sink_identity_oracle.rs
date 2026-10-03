//! Read-only extraction of exact existing frozen GNU .elc code/pools.
//! Immutable scalar test data, no Lisp ownership/cache state.
#[derive(Clone, Copy, Debug)]
pub(super) enum Constant {
    Integer(i64),
    Float(u64),
    Symbol(&'static str),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Function {
    pub name: &'static str,
    pub descriptor: i64,
    pub max_stack: u16,
    pub bytes: &'static [u8],
    pub constants: &'static [Constant],
}
pub(super) const FUNCTIONS: &[Function] = &[
    Function {
        name: "t34-o36-loop-keep",
        descriptor: 514,
        max_stack: 5,
        bytes: &[192, 137, 2, 87, 131, 11, 0, 84, 130, 1, 0, 2, 137, 66, 135],
        constants: &[Constant::Integer(0)],
    },
    Function {
        name: "t34-o36-loop-add",
        descriptor: 771,
        max_stack: 8,
        bytes: &[
            2, 137, 192, 137, 4, 87, 131, 21, 0, 2, 5, 92, 137, 178, 4, 178, 2, 84, 130, 3, 0, 2,
            2, 66, 135,
        ],
        constants: &[Constant::Integer(0)],
    },
    Function {
        name: "t34-o36-loop-lag",
        descriptor: 771,
        max_stack: 8,
        bytes: &[
            2, 137, 192, 137, 4, 87, 131, 21, 0, 2, 5, 92, 3, 178, 3, 178, 3, 84, 130, 3, 0, 2, 2,
            66, 135,
        ],
        constants: &[Constant::Integer(0)],
    },
    Function {
        name: "t34-o36-loop-mixed",
        descriptor: 514,
        max_stack: 7,
        bytes: &[
            1, 137, 192, 137, 4, 87, 131, 33, 0, 137, 193, 166, 192, 85, 131, 23, 0, 4, 1, 92, 130,
            24, 0, 137, 137, 178, 4, 178, 2, 84, 130, 3, 0, 2, 2, 66, 135,
        ],
        constants: &[Constant::Integer(0), Constant::Integer(2)],
    },
    Function {
        name: "t34-o36-loop-mixed-post",
        descriptor: 514,
        max_stack: 7,
        bytes: &[
            1, 137, 192, 137, 4, 87, 131, 33, 0, 137, 193, 166, 192, 85, 131, 23, 0, 4, 1, 92, 130,
            24, 0, 137, 137, 178, 4, 178, 2, 84, 130, 3, 0, 3, 192, 86, 131, 47, 0, 2, 192, 92,
            137, 178, 4, 178, 2, 2, 2, 66, 135,
        ],
        constants: &[Constant::Integer(0), Constant::Integer(2)],
    },
    Function {
        name: "t34-o36-loop-existing-new",
        descriptor: 514,
        max_stack: 7,
        bytes: &[
            1, 137, 192, 137, 4, 87, 131, 33, 0, 137, 193, 166, 192, 85, 131, 23, 0, 4, 194, 92,
            130, 24, 0, 4, 137, 178, 4, 178, 2, 84, 130, 3, 0, 2, 2, 66, 135,
        ],
        constants: &[
            Constant::Integer(0),
            Constant::Integer(2),
            Constant::Float(0x0000000000000000),
        ],
    },
    Function {
        name: "t34-o36-loop-partial-escape",
        descriptor: 771,
        max_stack: 9,
        bytes: &[
            2, 137, 192, 137, 5, 87, 131, 35, 0, 2, 193, 92, 137, 178, 4, 137, 178, 3, 1, 194, 166,
            192, 85, 131, 30, 0, 4, 4, 160, 136, 136, 84, 130, 3, 0, 2, 2, 66, 135,
        ],
        constants: &[
            Constant::Integer(0),
            Constant::Float(0x0000000000000000),
            Constant::Integer(2),
        ],
    },
    Function {
        name: "t34-o36-loop-gc-dual",
        descriptor: 514,
        max_stack: 8,
        bytes: &[
            1, 137, 192, 137, 4, 87, 131, 32, 0, 2, 193, 92, 137, 178, 4, 178, 2, 84, 137, 194,
            166, 192, 85, 131, 3, 0, 195, 32, 136, 130, 3, 0, 2, 2, 6, 6, 69, 135,
        ],
        constants: &[
            Constant::Integer(0),
            Constant::Float(0x0000000000000000),
            Constant::Integer(255),
            Constant::Symbol("garbage-collect"),
        ],
    },
    Function {
        name: "t34-o36-loop-gc-lag",
        descriptor: 514,
        max_stack: 8,
        bytes: &[
            1, 137, 192, 137, 4, 87, 131, 32, 0, 2, 193, 92, 3, 178, 3, 178, 3, 84, 137, 194, 166,
            192, 85, 131, 3, 0, 195, 32, 136, 130, 3, 0, 2, 2, 6, 6, 69, 135,
        ],
        constants: &[
            Constant::Integer(0),
            Constant::Float(0x0000000000000000),
            Constant::Integer(255),
            Constant::Symbol("garbage-collect"),
        ],
    },
    Function {
        name: "t34-o36-float-alias",
        descriptor: 514,
        max_stack: 11,
        bytes: &[
            1, 1, 92, 2, 2, 92, 1, 1, 192, 4, 4, 61, 5, 5, 85, 6, 6, 6, 9, 61, 175, 6, 135,
        ],
        constants: &[Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-float-escape",
        descriptor: 1028,
        max_stack: 13,
        bytes: &[
            3, 3, 92, 192, 3, 2, 160, 136, 2, 2, 33, 178, 1, 1, 137, 2, 193, 5, 6, 8, 64, 61, 6, 6,
            6, 6, 61, 175, 6, 135,
        ],
        constants: &[Constant::Symbol("nil"), Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-float-phi",
        descriptor: 1028,
        max_stack: 9,
        bytes: &[
            3, 131, 10, 0, 2, 2, 92, 130, 11, 0, 137, 137, 1, 137, 4, 61, 192, 70, 135,
        ],
        constants: &[Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-float-partial-escape",
        descriptor: 1028,
        max_stack: 10,
        bytes: &[
            2, 2, 92, 4, 131, 11, 0, 1, 1, 160, 136, 137, 1, 192, 6, 7, 133, 23, 0, 3, 5, 64, 61,
            70, 135,
        ],
        constants: &[Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-local-cons",
        descriptor: 514,
        max_stack: 5,
        bytes: &[1, 1, 66, 137, 64, 1, 65, 92, 135],
        constants: &[],
    },
    Function {
        name: "t34-o36-return-list1",
        descriptor: 257,
        max_stack: 2,
        bytes: &[67, 135],
        constants: &[],
    },
    Function {
        name: "t34-o36-cons-identity",
        descriptor: 514,
        max_stack: 9,
        bytes: &[
            1, 1, 66, 2, 2, 66, 192, 2, 2, 61, 3, 64, 6, 6, 61, 4, 65, 6, 6, 61, 70, 135,
        ],
        constants: &[Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-cons-escape",
        descriptor: 1028,
        max_stack: 13,
        bytes: &[
            3, 3, 66, 192, 3, 2, 160, 136, 2, 2, 33, 178, 1, 1, 137, 2, 193, 5, 6, 8, 64, 61, 6, 6,
            6, 6, 61, 175, 6, 135,
        ],
        constants: &[Constant::Symbol("nil"), Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-across-gc",
        descriptor: 1028,
        max_stack: 13,
        bytes: &[
            3, 3, 66, 2, 2, 92, 192, 32, 136, 1, 1, 193, 137, 5, 64, 6, 10, 61, 6, 6, 65, 6, 10,
            61, 175, 6, 135,
        ],
        constants: &[Constant::Symbol("garbage-collect"), Constant::Symbol("t")],
    },
    Function {
        name: "t34-o36-virtual-fail",
        descriptor: 1028,
        max_stack: 12,
        bytes: &[
            193, 1, 68, 16, 137, 3, 66, 3, 3, 92, 1, 1, 6, 7, 194, 87, 4, 4, 6, 7, 175, 6, 135,
        ],
        constants: &[
            Constant::Symbol("t34-o36-side"),
            Constant::Symbol("stored"),
            Constant::Integer(1),
        ],
    },
    Function {
        name: "t34-o36-mutate-fail",
        descriptor: 1028,
        max_stack: 9,
        bytes: &[
            137, 3, 66, 137, 3, 161, 136, 193, 1, 137, 4, 70, 16, 137, 5, 194, 87, 2, 65, 4, 70,
            135,
        ],
        constants: &[
            Constant::Symbol("t34-o36-side"),
            Constant::Symbol("mutated"),
            Constant::Integer(1),
        ],
    },
    Function {
        name: "t34-o36-arith-fail",
        descriptor: 771,
        max_stack: 7,
        bytes: &[137, 2, 66, 193, 2, 68, 16, 137, 3, 5, 92, 2, 69, 135],
        constants: &[
            Constant::Symbol("t34-o36-side"),
            Constant::Symbol("arith-stored"),
        ],
    },
    Function {
        name: "t34-o36-constructor-fail",
        descriptor: 514,
        max_stack: 4,
        bytes: &[193, 1, 68, 16, 137, 2, 64, 66, 135],
        constants: &[
            Constant::Symbol("t34-o36-side"),
            Constant::Symbol("operand-stored"),
        ],
    },
];
