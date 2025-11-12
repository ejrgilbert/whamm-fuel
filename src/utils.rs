use wirm::ir::id::{FunctionID, TypeID};
use wirm::ir::module::module_types::Types;
use wirm::Module;
use wirm::wasmparser::{BlockType, Operator};

// Determine pops/pushes for instruction
// returns (pops, pushes)
pub fn stack_effects(op: &Operator, wasm: &Module) -> (usize, usize) {
    // TODO -- work with all operators!
    return match op {
        Operator::If { blockty, .. } => {
            // TODO -- the block effects don't come into play until this block's END!
            // and really...it doesn't actually add anything to the stack. It can just
            // pop values and return what's already on the stack...
            // TODO -- look up semantics and handle this properly!!
            block_effects(1, &blockty, wasm)
        },
        Operator::Block {blockty, ..} => {
            // TODO -- the block effects don't come into play until this block's END!
            // and really...it doesn't actually add anything to the stack. It can just
            // pop values and return what's already on the stack...
            // TODO -- look up semantics and handle this properly!!
            block_effects(0, &blockty, wasm)
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
        Operator::Return => (0,0),
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
        Operator::I32Eqz |
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
        Operator::I64Eqz |
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
        Operator::RefEq |
        Operator::StructNew { .. } |
        Operator::StructNewDefault { .. } |
        Operator::StructGet { .. } |
        Operator::StructGetS { .. } |
        Operator::StructGetU { .. } |
        Operator::StructSet { .. } |
        Operator::ArrayNew { .. } |
        Operator::ArrayNewDefault { .. } |
        Operator::ArrayNewFixed { .. } |
        Operator::ArrayNewData { .. } |
        Operator::ArrayNewElem { .. } |
        Operator::ArrayGet { .. } |
        Operator::ArrayGetS { .. } |
        Operator::ArrayGetU { .. } |
        Operator::ArraySet { .. } |
        Operator::ArrayLen |
        Operator::ArrayFill { .. } |
        Operator::ArrayCopy { .. } |
        Operator::ArrayInitData { .. } |
        Operator::ArrayInitElem { .. } |
        Operator::RefTestNonNull { .. } |
        Operator::RefTestNullable { .. } |
        Operator::RefCastNonNull { .. } |
        Operator::RefCastNullable { .. } |
        Operator::BrOnCast { .. } |
        Operator::BrOnCastFail { .. } |
        Operator::AnyConvertExtern |
        Operator::ExternConvertAny |
        Operator::RefI31 |
        Operator::I31GetS |
        Operator::I31GetU |
        Operator::TypedSelect { .. } |
        Operator::TypedSelectMulti { .. } |
        Operator::RefNull { .. } |
        Operator::RefIsNull |
        Operator::RefFunc { .. } |
        Operator::RefI31Shared |
        Operator::RefAsNonNull |
        Operator::BrOnNull { .. } |
        Operator::BrOnNonNull { .. } => todo!(), // support GC opcodes
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
        Operator::TableFill { .. } |
        Operator::TableGet { .. } |
        Operator::TableSet { .. } |
        Operator::TableGrow { .. } |
        Operator::TableSize { .. } => todo!("support table ops"),
        Operator::MemoryDiscard { .. } |
        Operator::MemoryAtomicNotify { .. } |
        Operator::MemoryAtomicWait32 { .. } |
        Operator::MemoryAtomicWait64 { .. } => todo!("support memory ops"),
        Operator::AtomicFence |
        Operator::I32AtomicLoad { .. } |
        Operator::I64AtomicLoad { .. } |
        Operator::I32AtomicLoad8U { .. } |
        Operator::I32AtomicLoad16U { .. } |
        Operator::I64AtomicLoad8U { .. } |
        Operator::I64AtomicLoad16U { .. } |
        Operator::I64AtomicLoad32U { .. } |
        Operator::I32AtomicStore { .. } |
        Operator::I64AtomicStore { .. } |
        Operator::I32AtomicStore8 { .. } |
        Operator::I32AtomicStore16 { .. } |
        Operator::I64AtomicStore8 { .. } |
        Operator::I64AtomicStore16 { .. } |
        Operator::I64AtomicStore32 { .. } |
        Operator::I32AtomicRmwAdd { .. } |
        Operator::I64AtomicRmwAdd { .. } |
        Operator::I32AtomicRmw8AddU { .. } |
        Operator::I32AtomicRmw16AddU { .. } |
        Operator::I64AtomicRmw8AddU { .. } |
        Operator::I64AtomicRmw16AddU { .. } |
        Operator::I64AtomicRmw32AddU { .. } |
        Operator::I32AtomicRmwSub { .. } |
        Operator::I64AtomicRmwSub { .. } |
        Operator::I32AtomicRmw8SubU { .. } |
        Operator::I32AtomicRmw16SubU { .. } |
        Operator::I64AtomicRmw8SubU { .. } |
        Operator::I64AtomicRmw16SubU { .. } |
        Operator::I64AtomicRmw32SubU { .. } |
        Operator::I32AtomicRmwAnd { .. } |
        Operator::I64AtomicRmwAnd { .. } |
        Operator::I32AtomicRmw8AndU { .. } |
        Operator::I32AtomicRmw16AndU { .. } |
        Operator::I64AtomicRmw8AndU { .. } |
        Operator::I64AtomicRmw16AndU { .. } |
        Operator::I64AtomicRmw32AndU { .. } |
        Operator::I32AtomicRmwOr { .. } |
        Operator::I64AtomicRmwOr { .. } |
        Operator::I32AtomicRmw8OrU { .. } |
        Operator::I32AtomicRmw16OrU { .. } |
        Operator::I64AtomicRmw8OrU { .. } |
        Operator::I64AtomicRmw16OrU { .. } |
        Operator::I64AtomicRmw32OrU { .. } |
        Operator::I32AtomicRmwXor { .. } |
        Operator::I64AtomicRmwXor { .. } |
        Operator::I32AtomicRmw8XorU { .. } |
        Operator::I32AtomicRmw16XorU { .. } |
        Operator::I64AtomicRmw8XorU { .. } |
        Operator::I64AtomicRmw16XorU { .. } |
        Operator::I64AtomicRmw32XorU { .. } |
        Operator::I32AtomicRmwXchg { .. } |
        Operator::I64AtomicRmwXchg { .. } |
        Operator::I32AtomicRmw8XchgU { .. } |
        Operator::I32AtomicRmw16XchgU { .. } |
        Operator::I64AtomicRmw8XchgU { .. } |
        Operator::I64AtomicRmw16XchgU { .. } |
        Operator::I64AtomicRmw32XchgU { .. } |
        Operator::I32AtomicRmwCmpxchg { .. } |
        Operator::I64AtomicRmwCmpxchg { .. } |
        Operator::I32AtomicRmw8CmpxchgU { .. } |
        Operator::I32AtomicRmw16CmpxchgU { .. } |
        Operator::I64AtomicRmw8CmpxchgU { .. } |
        Operator::I64AtomicRmw16CmpxchgU { .. } |
        Operator::I64AtomicRmw32CmpxchgU { .. } |
        Operator::GlobalAtomicGet { .. } |
        Operator::GlobalAtomicSet { .. } |
        Operator::GlobalAtomicRmwAdd { .. } |
        Operator::GlobalAtomicRmwSub { .. } |
        Operator::GlobalAtomicRmwAnd { .. } |
        Operator::GlobalAtomicRmwOr { .. } |
        Operator::GlobalAtomicRmwXor { .. } |
        Operator::GlobalAtomicRmwXchg { .. } |
        Operator::GlobalAtomicRmwCmpxchg { .. } |
        Operator::TableAtomicGet { .. } |
        Operator::TableAtomicSet { .. } |
        Operator::TableAtomicRmwXchg { .. } |
        Operator::TableAtomicRmwCmpxchg { .. } |
        Operator::StructAtomicGet { .. } |
        Operator::StructAtomicGetS { .. } |
        Operator::StructAtomicGetU { .. } |
        Operator::StructAtomicSet { .. } |
        Operator::StructAtomicRmwAdd { .. } |
        Operator::StructAtomicRmwSub { .. } |
        Operator::StructAtomicRmwAnd { .. } |
        Operator::StructAtomicRmwOr { .. } |
        Operator::StructAtomicRmwXor { .. } |
        Operator::StructAtomicRmwXchg { .. } |
        Operator::StructAtomicRmwCmpxchg { .. } |
        Operator::ArrayAtomicGet { .. } |
        Operator::ArrayAtomicGetS { .. } |
        Operator::ArrayAtomicGetU { .. } |
        Operator::ArrayAtomicSet { .. } |
        Operator::ArrayAtomicRmwAdd { .. } |
        Operator::ArrayAtomicRmwSub { .. } |
        Operator::ArrayAtomicRmwAnd { .. } |
        Operator::ArrayAtomicRmwOr { .. } |
        Operator::ArrayAtomicRmwXor { .. } |
        Operator::ArrayAtomicRmwXchg { .. } |
        Operator::ArrayAtomicRmwCmpxchg { .. } => todo!("support atomic ops"),
        Operator::V128Load { .. } |
        Operator::V128Load8x8S { .. } |
        Operator::V128Load8x8U { .. } |
        Operator::V128Load16x4S { .. } |
        Operator::V128Load16x4U { .. } |
        Operator::V128Load32x2S { .. } |
        Operator::V128Load32x2U { .. } |
        Operator::V128Load8Splat { .. } |
        Operator::V128Load16Splat { .. } |
        Operator::V128Load32Splat { .. } |
        Operator::V128Load64Splat { .. } |
        Operator::V128Load32Zero { .. } |
        Operator::V128Load64Zero { .. } |
        Operator::V128Store { .. } |
        Operator::V128Load8Lane { .. } |
        Operator::V128Load16Lane { .. } |
        Operator::V128Load32Lane { .. } |
        Operator::V128Load64Lane { .. } |
        Operator::V128Store8Lane { .. } |
        Operator::V128Store16Lane { .. } |
        Operator::V128Store32Lane { .. } |
        Operator::V128Store64Lane { .. } |
        Operator::V128Const { .. } |
        Operator::I8x16Shuffle { .. } |
        Operator::I8x16ExtractLaneS { .. } |
        Operator::I8x16ExtractLaneU { .. } |
        Operator::I8x16ReplaceLane { .. } |
        Operator::I16x8ExtractLaneS { .. } |
        Operator::I16x8ExtractLaneU { .. } |
        Operator::I16x8ReplaceLane { .. } |
        Operator::I32x4ExtractLane { .. } |
        Operator::I32x4ReplaceLane { .. } |
        Operator::I64x2ExtractLane { .. } |
        Operator::I64x2ReplaceLane { .. } |
        Operator::F32x4ExtractLane { .. } |
        Operator::F32x4ReplaceLane { .. } |
        Operator::F64x2ExtractLane { .. } |
        Operator::F64x2ReplaceLane { .. } |
        Operator::I8x16Swizzle |
        Operator::I8x16Splat |
        Operator::I16x8Splat |
        Operator::I32x4Splat |
        Operator::I64x2Splat |
        Operator::F32x4Splat |
        Operator::F64x2Splat |
        Operator::I8x16Eq |
        Operator::I8x16Ne |
        Operator::I8x16LtS |
        Operator::I8x16LtU |
        Operator::I8x16GtS |
        Operator::I8x16GtU |
        Operator::I8x16LeS |
        Operator::I8x16LeU |
        Operator::I8x16GeS |
        Operator::I8x16GeU |
        Operator::I16x8Eq |
        Operator::I16x8Ne |
        Operator::I16x8LtS |
        Operator::I16x8LtU |
        Operator::I16x8GtS |
        Operator::I16x8GtU |
        Operator::I16x8LeS |
        Operator::I16x8LeU |
        Operator::I16x8GeS |
        Operator::I16x8GeU |
        Operator::I32x4Eq |
        Operator::I32x4Ne |
        Operator::I32x4LtS |
        Operator::I32x4LtU |
        Operator::I32x4GtS |
        Operator::I32x4GtU |
        Operator::I32x4LeS |
        Operator::I32x4LeU |
        Operator::I32x4GeS |
        Operator::I32x4GeU |
        Operator::I64x2Eq |
        Operator::I64x2Ne |
        Operator::I64x2LtS |
        Operator::I64x2GtS |
        Operator::I64x2LeS |
        Operator::I64x2GeS |
        Operator::F32x4Eq |
        Operator::F32x4Ne |
        Operator::F32x4Lt |
        Operator::F32x4Gt |
        Operator::F32x4Le |
        Operator::F32x4Ge |
        Operator::F64x2Eq |
        Operator::F64x2Ne |
        Operator::F64x2Lt |
        Operator::F64x2Gt |
        Operator::F64x2Le |
        Operator::F64x2Ge |
        Operator::V128Not |
        Operator::V128And |
        Operator::V128AndNot |
        Operator::V128Or |
        Operator::V128Xor |
        Operator::V128Bitselect |
        Operator::V128AnyTrue |
        Operator::I8x16Abs |
        Operator::I8x16Neg |
        Operator::I8x16Popcnt |
        Operator::I8x16AllTrue |
        Operator::I8x16Bitmask |
        Operator::I8x16NarrowI16x8S |
        Operator::I8x16NarrowI16x8U |
        Operator::I8x16Shl |
        Operator::I8x16ShrS |
        Operator::I8x16ShrU |
        Operator::I8x16Add |
        Operator::I8x16AddSatS |
        Operator::I8x16AddSatU |
        Operator::I8x16Sub |
        Operator::I8x16SubSatS |
        Operator::I8x16SubSatU |
        Operator::I8x16MinS |
        Operator::I8x16MinU |
        Operator::I8x16MaxS |
        Operator::I8x16MaxU |
        Operator::I8x16AvgrU |
        Operator::I16x8ExtAddPairwiseI8x16S |
        Operator::I16x8ExtAddPairwiseI8x16U |
        Operator::I16x8Abs |
        Operator::I16x8Neg |
        Operator::I16x8Q15MulrSatS |
        Operator::I16x8AllTrue |
        Operator::I16x8Bitmask |
        Operator::I16x8NarrowI32x4S |
        Operator::I16x8NarrowI32x4U |
        Operator::I16x8ExtendLowI8x16S |
        Operator::I16x8ExtendHighI8x16S |
        Operator::I16x8ExtendLowI8x16U |
        Operator::I16x8ExtendHighI8x16U |
        Operator::I16x8Shl |
        Operator::I16x8ShrS |
        Operator::I16x8ShrU |
        Operator::I16x8Add |
        Operator::I16x8AddSatS |
        Operator::I16x8AddSatU |
        Operator::I16x8Sub |
        Operator::I16x8SubSatS |
        Operator::I16x8SubSatU |
        Operator::I16x8Mul |
        Operator::I16x8MinS |
        Operator::I16x8MinU |
        Operator::I16x8MaxS |
        Operator::I16x8MaxU |
        Operator::I16x8AvgrU |
        Operator::I16x8ExtMulLowI8x16S |
        Operator::I16x8ExtMulHighI8x16S |
        Operator::I16x8ExtMulLowI8x16U |
        Operator::I16x8ExtMulHighI8x16U |
        Operator::I32x4ExtAddPairwiseI16x8S |
        Operator::I32x4ExtAddPairwiseI16x8U |
        Operator::I32x4Abs |
        Operator::I32x4Neg |
        Operator::I32x4AllTrue |
        Operator::I32x4Bitmask |
        Operator::I32x4ExtendLowI16x8S |
        Operator::I32x4ExtendHighI16x8S |
        Operator::I32x4ExtendLowI16x8U |
        Operator::I32x4ExtendHighI16x8U |
        Operator::I32x4Shl |
        Operator::I32x4ShrS |
        Operator::I32x4ShrU |
        Operator::I32x4Add |
        Operator::I32x4Sub |
        Operator::I32x4Mul |
        Operator::I32x4MinS |
        Operator::I32x4MinU |
        Operator::I32x4MaxS |
        Operator::I32x4MaxU |
        Operator::I32x4DotI16x8S |
        Operator::I32x4ExtMulLowI16x8S |
        Operator::I32x4ExtMulHighI16x8S |
        Operator::I32x4ExtMulLowI16x8U |
        Operator::I32x4ExtMulHighI16x8U |
        Operator::I64x2Abs |
        Operator::I64x2Neg |
        Operator::I64x2AllTrue |
        Operator::I64x2Bitmask |
        Operator::I64x2ExtendLowI32x4S |
        Operator::I64x2ExtendHighI32x4S |
        Operator::I64x2ExtendLowI32x4U |
        Operator::I64x2ExtendHighI32x4U |
        Operator::I64x2Shl |
        Operator::I64x2ShrS |
        Operator::I64x2ShrU |
        Operator::I64x2Add |
        Operator::I64x2Sub |
        Operator::I64x2Mul |
        Operator::I64x2ExtMulLowI32x4S |
        Operator::I64x2ExtMulHighI32x4S |
        Operator::I64x2ExtMulLowI32x4U |
        Operator::I64x2ExtMulHighI32x4U |
        Operator::F32x4Ceil |
        Operator::F32x4Floor |
        Operator::F32x4Trunc |
        Operator::F32x4Nearest |
        Operator::F32x4Abs |
        Operator::F32x4Neg |
        Operator::F32x4Sqrt |
        Operator::F32x4Add |
        Operator::F32x4Sub |
        Operator::F32x4Mul |
        Operator::F32x4Div |
        Operator::F32x4Min |
        Operator::F32x4Max |
        Operator::F32x4PMin |
        Operator::F32x4PMax |
        Operator::F64x2Ceil |
        Operator::F64x2Floor |
        Operator::F64x2Trunc |
        Operator::F64x2Nearest |
        Operator::F64x2Abs |
        Operator::F64x2Neg |
        Operator::F64x2Sqrt |
        Operator::F64x2Add |
        Operator::F64x2Sub |
        Operator::F64x2Mul |
        Operator::F64x2Div |
        Operator::F64x2Min |
        Operator::F64x2Max |
        Operator::F64x2PMin |
        Operator::F64x2PMax |
        Operator::I32x4TruncSatF32x4S |
        Operator::I32x4TruncSatF32x4U |
        Operator::F32x4ConvertI32x4S |
        Operator::F32x4ConvertI32x4U |
        Operator::I32x4TruncSatF64x2SZero |
        Operator::I32x4TruncSatF64x2UZero |
        Operator::F64x2ConvertLowI32x4S |
        Operator::F64x2ConvertLowI32x4U |
        Operator::F32x4DemoteF64x2Zero |
        Operator::F64x2PromoteLowF32x4 |
        Operator::I8x16RelaxedSwizzle |
        Operator::I32x4RelaxedTruncF32x4S |
        Operator::I32x4RelaxedTruncF32x4U |
        Operator::I32x4RelaxedTruncF64x2SZero |
        Operator::I32x4RelaxedTruncF64x2UZero |
        Operator::F32x4RelaxedMadd |
        Operator::F32x4RelaxedNmadd |
        Operator::F64x2RelaxedMadd |
        Operator::F64x2RelaxedNmadd |
        Operator::I8x16RelaxedLaneselect |
        Operator::I16x8RelaxedLaneselect |
        Operator::I32x4RelaxedLaneselect |
        Operator::I64x2RelaxedLaneselect |
        Operator::F32x4RelaxedMin |
        Operator::F32x4RelaxedMax |
        Operator::F64x2RelaxedMin |
        Operator::F64x2RelaxedMax |
        Operator::I16x8RelaxedQ15mulrS |
        Operator::I16x8RelaxedDotI8x16I7x16S |
        Operator::I32x4RelaxedDotI8x16I7x16AddS |
        Operator::I64Add128 |
        Operator::I64Sub128 |
        Operator::I64MulWideS |
        Operator::I64MulWideU => todo!("support SIMD ops"),
        Operator::TryTable { .. } => todo!(),
        Operator::Throw { .. } |
        Operator::ThrowRef |
        Operator::Try { .. } |
        Operator::Catch { .. } |
        Operator::Rethrow { .. } |
        Operator::Delegate { .. } |
        Operator::CatchAll => todo!("support exception ops"),
        Operator::ContNew { .. } |
        Operator::ContBind { .. } |
        Operator::Suspend { .. } |
        Operator::Resume { .. } |
        Operator::ResumeThrow { .. } |
        Operator::Switch { .. } => todo!("support stack switching"),
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
