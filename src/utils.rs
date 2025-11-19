use wirm::ir::id::{FunctionID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::Module;
use wirm::wasmparser::{BlockType, Operator};
use crate::codegen::CompType;

pub(crate) const SPACE_PER_TAB: usize = 4;
pub(crate) const INIT_FUEL: i64 = 1000;
pub(crate) const FUEL_COMPUTATION: CompType = CompType::Exact;

pub fn is_branching_op(op: &Operator) -> bool {
    matches!(op, Operator::Br {..} | Operator::BrIf{..} | Operator::BrTable{..} |
                 Operator::BrOnCast {..} | Operator::BrOnCastFail {..} |  Operator::BrOnNonNull {..} |
                 Operator::BrOnNull {..})
}

// Determine pops/pushes for instruction
// returns (pops, pushes)
pub fn stack_effects(op: &Operator, wasm: &Module) -> (usize, usize) {
    return match op {
        Operator::If { blockty, .. } => {
            // NOTE: it doesn't actually add anything to the stack. It can just
            // pop values and return what's already on the stack...
            block_effects(1, blockty, wasm)
        },
        Operator::Block {blockty, ..} => {
            // NOTE: it doesn't actually add anything to the stack. It can just
            // pop values and return what's already on the stack...
            block_effects(0, blockty, wasm)
        },
        Operator::BrIf { .. } |
        Operator::BrTable { .. } => (1, 0),
        Operator::Call { function_index } |
        Operator::ReturnCall { function_index } => {
            let tid = wasm.functions.get(FunctionID(*function_index)).get_type_id();
            ty_effects(0, *tid, wasm)
        }
        Operator::CallIndirect { type_index, .. } |
        Operator::ReturnCallIndirect { type_index, .. } |
        Operator::CallRef { type_index } |
        Operator::ReturnCallRef { type_index } => ty_effects(1, *type_index, wasm),
        Operator::LocalGet { .. } => (0, 1),
        Operator::LocalSet { .. } => (1, 0),
        Operator::LocalTee { .. } => (1, 1),
        Operator::GlobalGet { .. } => (0, 1),
        Operator::GlobalSet { .. } => (1, 0),
        Operator::I32Const { .. } | Operator::I64Const { .. } | Operator::F32Const { .. } | Operator::F64Const { .. } => (0,1),
        Operator::I32Load { .. } | Operator::I64Load { .. } | Operator::F32Load { .. } | Operator::F64Load { .. } => (1,1),
        Operator::I32Store { .. } | Operator::I64Store { .. } | Operator::F32Store { .. } | Operator::F64Store { .. } => (2,0),
        Operator::Drop { .. } => (1, 0),
        // _ => (0,0),
        Operator::Unreachable => (0, 0),
        Operator::Nop => (0, 0),
        Operator::Loop { blockty } => {
            match blockty {
                BlockType::Empty => (0,0),
                BlockType::FuncType(type_index) => ty_effects(0, *type_index, wasm),
                BlockType::Type(_) => (0, 1)
            }
        },
        Operator::Else => (0,0),
        Operator::End => (0,0),
        Operator::Br { .. } => (0,0),
        Operator::Return => unreachable!(),
        Operator::Select => (3, 1),
        Operator::I32Load8S { .. } |
        Operator::I32Load8U { .. } |
        Operator::I32Load16S { .. } |
        Operator::I32Load16U { .. } |
        Operator::I64Load8S { .. } |
        Operator::I64Load8U { .. } |
        Operator::I64Load16S { .. } |
        Operator::I64Load16U { .. } |
        Operator::I64Load32S { .. } |
        Operator::I64Load32U { .. } => (1, 1),
        Operator::I32Store8 { .. } |
        Operator::I32Store16 { .. } |
        Operator::I64Store8 { .. } |
        Operator::I64Store16 { .. } |
        Operator::I64Store32 { .. } => (1, 0),
        Operator::MemorySize { .. } => (0, 1),
        Operator::MemoryGrow { .. } => (1, 1),
        Operator::I32Eq |
        Operator::I32Ne |
        Operator::I32LtS |
        Operator::I32LtU |
        Operator::I32GtS |
        Operator::I32GtU |
        Operator::I32LeS |
        Operator::I32LeU |
        Operator::I32GeS |
        Operator::I32GeU |
        Operator::I64Eq |
        Operator::I64Ne |
        Operator::I64LtS |
        Operator::I64LtU |
        Operator::I64GtS |
        Operator::I64GtU |
        Operator::I64LeS |
        Operator::I64LeU |
        Operator::I64GeS |
        Operator::I64GeU |
        Operator::F32Eq |
        Operator::F32Ne |
        Operator::F32Lt |
        Operator::F32Gt |
        Operator::F32Le |
        Operator::F32Ge |
        Operator::F64Eq |
        Operator::F64Ne |
        Operator::F64Lt |
        Operator::F64Gt |
        Operator::F64Le |
        Operator::F64Ge => (2, 1),
        Operator::I32Eqz |
        Operator::I64Eqz |
        Operator::I32Clz |
        Operator::I32Ctz |
        Operator::I32Popcnt => (1, 1),
        Operator::I32Add |
        Operator::I32Sub |
        Operator::I32Mul |
        Operator::I32DivS |
        Operator::I32DivU |
        Operator::I32RemS |
        Operator::I32RemU |
        Operator::I32And |
        Operator::I32Or |
        Operator::I32Xor |
        Operator::I32Shl |
        Operator::I32ShrS |
        Operator::I32ShrU |
        Operator::I32Rotl |
        Operator::I32Rotr => (2, 1),
        Operator::I64Clz |
        Operator::I64Ctz |
        Operator::I64Popcnt => (1, 1),
        Operator::I64Add |
        Operator::I64Sub |
        Operator::I64Mul |
        Operator::I64DivS |
        Operator::I64DivU |
        Operator::I64RemS |
        Operator::I64RemU |
        Operator::I64And |
        Operator::I64Or |
        Operator::I64Xor |
        Operator::I64Shl |
        Operator::I64ShrS |
        Operator::I64ShrU |
        Operator::I64Rotl |
        Operator::I64Rotr => (2, 1),
        Operator::F32Abs |
        Operator::F32Neg |
        Operator::F32Ceil |
        Operator::F32Floor |
        Operator::F32Trunc |
        Operator::F32Nearest |
        Operator::F32Sqrt => (1, 1),
        Operator::F32Add |
        Operator::F32Sub |
        Operator::F32Mul |
        Operator::F32Div |
        Operator::F32Min |
        Operator::F32Max |
        Operator::F32Copysign => (2, 1),
        Operator::F64Abs |
        Operator::F64Neg |
        Operator::F64Ceil |
        Operator::F64Floor |
        Operator::F64Trunc |
        Operator::F64Nearest |
        Operator::F64Sqrt => (1, 1),
        Operator::F64Add |
        Operator::F64Sub |
        Operator::F64Mul |
        Operator::F64Div |
        Operator::F64Min |
        Operator::F64Max |
        Operator::F64Copysign => (2, 1),
        Operator::I32WrapI64 |
        Operator::I32TruncF32S |
        Operator::I32TruncF32U |
        Operator::I32TruncF64S |
        Operator::I32TruncF64U |
        Operator::I64ExtendI32S |
        Operator::I64ExtendI32U |
        Operator::I64TruncF32S |
        Operator::I64TruncF32U |
        Operator::I64TruncF64S |
        Operator::I64TruncF64U |
        Operator::F32ConvertI32S |
        Operator::F32ConvertI32U |
        Operator::F32ConvertI64S |
        Operator::F32ConvertI64U |
        Operator::F32DemoteF64 |
        Operator::F64ConvertI32S |
        Operator::F64ConvertI32U |
        Operator::F64ConvertI64S |
        Operator::F64ConvertI64U |
        Operator::F64PromoteF32 |
        Operator::I32ReinterpretF32 |
        Operator::I64ReinterpretF64 |
        Operator::F32ReinterpretI32 |
        Operator::F64ReinterpretI64 |
        Operator::I32Extend8S |
        Operator::I32Extend16S |
        Operator::I64Extend8S |
        Operator::I64Extend16S |
        Operator::I64Extend32S => (1, 1),
        Operator::I32TruncSatF32S |
        Operator::I32TruncSatF32U |
        Operator::I32TruncSatF64S |
        Operator::I32TruncSatF64U |
        Operator::I64TruncSatF32S |
        Operator::I64TruncSatF32U |
        Operator::I64TruncSatF64S |
        Operator::I64TruncSatF64U => (1, 1),
        Operator::MemoryInit { .. } |
        Operator::MemoryCopy { .. } |
        Operator::MemoryFill { .. } |
        Operator::TableInit { .. } |
        Operator::TableCopy { .. } => (3, 1),
        Operator::DataDrop { .. } |
        Operator::ElemDrop { .. } => (0, 0),
        _ => todo!("op not supported: {op:?}")
    };

    fn block_effects(extra_pop: usize, blockty: &BlockType, wasm: &Module) -> (usize, usize) {
        match blockty {
            BlockType::Empty => (extra_pop, 0),
            BlockType::Type(_) => (extra_pop, 1),
            BlockType::FuncType(tid) => ty_effects(extra_pop, *tid, wasm),
        }
    }
    fn ty_effects(extra_pop: usize, tid: u32, wasm: &Module) -> (usize, usize) {
        if let Some(Types::FuncType { params , results, ..}) = wasm.types.get(TypeID(tid)) {
            (params.len() + extra_pop, results.len())
        } else {
            panic!("Should have found a function type!");
        }
    }
}
