// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

pub(crate) mod psd2;
pub(crate) use psd2::*;

pub(crate) mod psd4;
pub(crate) use psd4::*;

pub(crate) mod psd8;
pub(crate) use psd8::*;

use crate::{
    function::{parsers::*, Instruction, Opcode, Operation, Registers},
    helpers::Register,
    Program,
    Value,
};
use snarkvm_circuits::{
    Aleo,
    Environment,
    FromBits,
    Literal,
    Parser,
    ParserResult,
    PrimeField,
    ToBits,
    ToField,
    ToGroup,
};
use snarkvm_utilities::{FromBytes, ToBytes};

use core::{fmt, marker::PhantomData};
use nom::combinator::map;
use std::io::{Read, Result as IoResult, Write};

pub trait PRFOpcode {
    const OPCODE: &'static str;
}

/// A generic PRF instruction.
#[allow(clippy::upper_case_acronyms)]
pub struct PRF<P: Program, Op: PRFOpcode> {
    operation: BinaryOperation<P>,
    _phantom: PhantomData<Op>,
}

impl<P: Program, Op: PRFOpcode> Opcode for PRF<P, Op> {
    /// Returns the opcode as a string.
    #[inline]
    fn opcode() -> &'static str {
        Op::OPCODE
    }
}

impl<P: Program, Op: PRFOpcode> PRF<P, Op> {
    /// Returns the operands of the instruction.
    pub fn operands(&self) -> Vec<Operand<P>> {
        self.operation.operands()
    }

    /// Returns the destination register of the instruction.
    pub fn destination(&self) -> &Register<P> {
        self.operation.destination()
    }
}

impl<P: Program, Op: PRFOpcode> Operation<P> for PRF<P, Op> {
    /// Evaluates the operation.
    #[inline]
    fn evaluate(&self, registers: &Registers<P>) {
        // Load the seed from the operand.
        let first = match registers.load(self.operation.first()) {
            Value::Literal(literal) => literal,
            Value::Composite(name, ..) => P::halt(format!("{name} is not a literal")),
        };
        // Load the input from the operand.
        let second = match registers.load(self.operation.second()) {
            Value::Literal(literal) => vec![literal],
            Value::Composite(_name, literals) => literals,
        };

        // Ensure `first` is a Field.
        let first = match first {
            Literal::Field(field) => field,
            _ => P::halt("Unreachable literal variant detected during PRF calculation."),
        };
        // TODO (howardwu): Implement `Literal::to_fields()` to replace this closure.
        // (Optional) Closure for converting a list of literals into a list of field elements.
        //
        // If the list is comprised of `Address`, `Field`, `Group`, and/or `Scalar`, then the closure
        // will return the underlying field elements (instead of packing the literals from bits).
        // Otherwise, the list is converted into bits, and then packed into field elements.
        let to_field_elements = |input: &[Literal<_>]| {
            // Determine whether the input is comprised of field-friendly literals.
            match input.iter().all(|literal| {
                matches!(literal, Literal::Address(_) | Literal::Field(_) | Literal::Group(_) | Literal::Scalar(_))
            }) {
                // Case 1 - Map each literal directly to its field representation.
                true => input
                    .iter()
                    .map(|literal| match literal {
                        Literal::Address(address) => address.to_group().to_x_coordinate(),
                        Literal::Field(field) => field.clone(),
                        Literal::Group(group) => group.to_x_coordinate(),
                        Literal::Scalar(scalar) => scalar.to_field(),
                        _ => P::halt("Unreachable literal variant detected during PRF calculation."),
                    })
                    .collect::<Vec<_>>(),
                // Case 2 - Convert the literals to bits, and then pack them into field elements.
                false => input
                    .to_bits_le()
                    .chunks(<P::Aleo as Environment>::BaseField::size_in_data_bits())
                    .map(FromBits::from_bits_le)
                    .collect::<Vec<_>>(),
            }
        };

        // Compute the digest for the given input.
        let digest = match Self::opcode() {
            Psd2::OPCODE => P::Aleo::prf_psd2(&first, &to_field_elements(&second)),
            Psd4::OPCODE => P::Aleo::prf_psd4(&first, &to_field_elements(&second)),
            Psd8::OPCODE => P::Aleo::prf_psd8(&first, &to_field_elements(&second)),
            _ => P::halt("Invalid option provided for the `prf` instruction"),
        };

        registers.assign(self.operation.destination(), Literal::Field(digest));
    }
}

impl<P: Program, Op: PRFOpcode> fmt::Display for PRF<P, Op> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.operation)
    }
}

impl<P: Program, Op: PRFOpcode> Parser for PRF<P, Op> {
    type Environment = P::Environment;

    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        map(BinaryOperation::parse, |operation| Self { operation, _phantom: PhantomData })(string)
    }
}

impl<P: Program, Op: PRFOpcode> FromBytes for PRF<P, Op> {
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        Ok(Self { operation: BinaryOperation::read_le(&mut reader)?, _phantom: PhantomData })
    }
}

impl<P: Program, Op: PRFOpcode> ToBytes for PRF<P, Op> {
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        self.operation.write_le(&mut writer)
    }
}

#[allow(clippy::from_over_into)]
impl<P: Program, Op: PRFOpcode> Into<Instruction<P>> for PRF<P, Op> {
    /// Converts the operation into an instruction.
    fn into(self) -> Instruction<P> {
        match Self::opcode() {
            Psd2::OPCODE => Instruction::PRFPsd2(PRFPsd2 { operation: self.operation, _phantom: PhantomData }),
            Psd4::OPCODE => Instruction::PRFPsd4(PRFPsd4 { operation: self.operation, _phantom: PhantomData }),
            Psd8::OPCODE => Instruction::PRFPsd8(PRFPsd8 { operation: self.operation, _phantom: PhantomData }),
            _ => P::halt("Invalid option provided for the `prf` instruction"),
        }
    }
}