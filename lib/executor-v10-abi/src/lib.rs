#![no_std]

pub const OPCODE: u8 = 61;
pub const MAX_COMPUTE_UNIT_LIMIT: u32 = 1_400_000;
pub const PREFIX_LEN: usize = 25;
pub const MAX_GROUPS: usize = 4;
pub const MAX_POOLS: usize = 16;
pub const MAX_POOL_ACCOUNTS: usize = 24;
pub const MAX_PAYLOAD_LEN: usize = PREFIX_LEN + 1 + MAX_GROUPS * 2 + MAX_POOLS * 2;
pub const MAX_ADDITIONAL_FEE_BP: u16 = 8_500;

pub mod cost;
pub use cost::Venue;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub minimum_profit: u64,
    /// Explicit allowance for V10; zero cannot be inferred from the runtime.
    pub compute_unit_limit: u32,
    pub no_failure: bool,
    pub additional_fee_bp: u16,
    pub use_flashloan: bool,
    pub trade_size: u64,
    pub constant_conversion: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Group {
    pub bridge_count: u8,
    pub direct_count: u8,
    /// With bridges: intermediate, bridges, directs. Without bridges: direct pools only.
    /// Each count includes the program and quote mint.
    pub pool_account_counts: [u16; MAX_POOLS],
}

impl Group {
    pub fn is_direct(&self) -> bool {
        self.bridge_count == 0
    }

    pub fn pool_count(&self) -> usize {
        usize::from(!self.is_direct()) + self.bridge_count as usize + self.direct_count as usize
    }

    /// Accounts before the group's pools: the target's mint, token program and
    /// wallet, and the base's too for bridged groups.
    pub fn header_account_count(&self) -> usize {
        if self.is_direct() {
            3
        } else {
            6
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstructionData {
    pub header: Header,
    /// Zero retains the original layout. Otherwise adds Q mint/program/wallet and one Q/S pool.
    pub conversion_account_count: u16,
    pub group_count: u8,
    pub groups: [Group; MAX_GROUPS],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    InvalidData,
}

impl InstructionData {
    pub fn validate(&self) -> Result<(), DecodeError> {
        if !(1..=MAX_COMPUTE_UNIT_LIMIT).contains(&self.header.compute_unit_limit)
            || self.header.additional_fee_bp > MAX_ADDITIONAL_FEE_BP
            || self.group_count == 0
            || self.group_count as usize > MAX_GROUPS
        {
            return Err(DecodeError::InvalidData);
        }
        if self.header.constant_conversion && self.conversion_account_count == 0 {
            return Err(DecodeError::InvalidData);
        }
        let mut pools = usize::from(self.conversion_account_count != 0);
        if self.conversion_account_count != 0
            && !(3..=MAX_POOL_ACCOUNTS as u16).contains(&self.conversion_account_count)
        {
            return Err(DecodeError::InvalidData);
        }
        for group in &self.groups[..self.group_count as usize] {
            pools += group.pool_count();
            if group.direct_count == 0
                || (group.is_direct() && group.direct_count < 2)
                || pools > MAX_POOLS
            {
                return Err(DecodeError::InvalidData);
            }
            for &count in &group.pool_account_counts[..group.pool_count()] {
                if count < 3 || count as usize > MAX_POOL_ACCOUNTS {
                    return Err(DecodeError::InvalidData);
                }
            }
            if group.pool_account_counts[group.pool_count()..]
                .iter()
                .any(|&n| n != 0)
            {
                return Err(DecodeError::InvalidData);
            }
        }
        if self.groups[self.group_count as usize..]
            .iter()
            .any(|g| *g != Group::default())
        {
            return Err(DecodeError::InvalidData);
        }
        Ok(())
    }

    pub fn decode(data: &[u8]) -> Result<Self, DecodeError> {
        let invalid = DecodeError::InvalidData;
        if data.len() < PREFIX_LEN + 1 || data[12] > 1 || data[15] > 1 || data[24] > 3 {
            return Err(invalid);
        }
        let mut result = Self {
            header: Header {
                minimum_profit: u64::from_le_bytes(data[0..8].try_into().map_err(|_| invalid)?),
                compute_unit_limit: u32::from_le_bytes(
                    data[8..12].try_into().map_err(|_| invalid)?,
                ),
                no_failure: data[12] != 0,
                additional_fee_bp: u16::from_le_bytes(
                    data[13..15].try_into().map_err(|_| invalid)?,
                ),
                use_flashloan: data[15] != 0,
                trade_size: u64::from_le_bytes(data[16..24].try_into().map_err(|_| invalid)?),
                constant_conversion: data[24] & 2 != 0,
            },
            group_count: data[25],
            ..Self::default()
        };
        if result.group_count as usize > MAX_GROUPS {
            return Err(invalid);
        }
        let mut offset = 26;
        if data[24] & 1 != 0 {
            result.conversion_account_count = u16::from_le_bytes(
                data.get(offset..offset + 2)
                    .ok_or(invalid)?
                    .try_into()
                    .map_err(|_| invalid)?,
            );
            if result.conversion_account_count == 0 {
                return Err(invalid);
            }
            offset += 2;
        }
        for group in &mut result.groups[..result.group_count as usize] {
            let counts = data.get(offset..offset + 2).ok_or(invalid)?;
            group.bridge_count = counts[0];
            group.direct_count = counts[1];
            offset += 2;
            if group.pool_count() > MAX_POOLS {
                return Err(invalid);
            }
            for i in 0..group.pool_count() {
                group.pool_account_counts[i] = u16::from_le_bytes(
                    data.get(offset..offset + 2)
                        .ok_or(invalid)?
                        .try_into()
                        .map_err(|_| invalid)?,
                );
                offset += 2;
            }
        }
        if offset != data.len() {
            return Err(invalid);
        }
        result.validate()?;
        Ok(result)
    }

    pub fn encode<'a>(
        &self,
        buffer: &'a mut [u8; MAX_PAYLOAD_LEN],
    ) -> Result<&'a [u8], DecodeError> {
        self.validate()?;
        let h = self.header;
        buffer[..8].copy_from_slice(&h.minimum_profit.to_le_bytes());
        buffer[8..12].copy_from_slice(&h.compute_unit_limit.to_le_bytes());
        buffer[12] = h.no_failure as u8;
        buffer[13..15].copy_from_slice(&h.additional_fee_bp.to_le_bytes());
        buffer[15] = h.use_flashloan as u8;
        buffer[16..24].copy_from_slice(&h.trade_size.to_le_bytes());
        buffer[24] =
            u8::from(self.conversion_account_count != 0) | (u8::from(h.constant_conversion) << 1);
        buffer[25] = self.group_count;
        let mut offset = 26;
        if self.conversion_account_count != 0 {
            buffer[offset..offset + 2]
                .copy_from_slice(&self.conversion_account_count.to_le_bytes());
            offset += 2;
        }
        for group in &self.groups[..self.group_count as usize] {
            buffer[offset] = group.bridge_count;
            buffer[offset + 1] = group.direct_count;
            offset += 2;
            for &count in &group.pool_account_counts[..group.pool_count()] {
                buffer[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
                offset += 2;
            }
        }
        Ok(&buffer[..offset])
    }

    /// Accounts before the first group: the fixed prefix and any conversion.
    fn prefix_account_count(&self) -> usize {
        7 + usize::from(self.header.additional_fee_bp != 0)
            + 2 * usize::from(self.header.use_flashloan)
            + if self.conversion_account_count != 0 {
                3 + self.conversion_account_count as usize
            } else {
                0
            }
    }

    /// Where each group's pools start in the account list, in instruction
    /// order. Each block begins with the pool's program.
    pub fn pool_blocks(&self) -> PoolBlocks<'_> {
        PoolBlocks {
            data: self,
            group: 0,
            index: 0,
            offset: self.prefix_account_count(),
        }
    }

    /// Accounts the instruction expects. `decode` and `encode` validate the
    /// counts first; this does not repeat that, and never panics on
    /// unvalidated counts.
    pub fn account_count(&self) -> usize {
        self.prefix_account_count()
            + self
                .groups
                .iter()
                .take(self.group_count as usize)
                .map(|g| {
                    g.header_account_count()
                        + g.pool_account_counts
                            .iter()
                            .take(g.pool_count())
                            .map(|&n| n as usize)
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}

/// A pool's position in the instruction: its group, its index within the
/// group, and the account index of its program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolBlock {
    pub group: usize,
    pub index: usize,
    pub offset: usize,
}

pub struct PoolBlocks<'a> {
    data: &'a InstructionData,
    group: usize,
    index: usize,
    offset: usize,
}

impl Iterator for PoolBlocks<'_> {
    type Item = PoolBlock;

    fn next(&mut self) -> Option<PoolBlock> {
        let groups = &self.data.groups[..(self.data.group_count as usize).min(MAX_GROUPS)];
        loop {
            let group = groups.get(self.group)?;
            if self.index == 0 {
                self.offset += group.header_account_count();
            }
            if self.index >= group.pool_count() {
                self.group += 1;
                self.index = 0;
                continue;
            }
            let block = PoolBlock {
                group: self.group,
                index: self.index,
                offset: self.offset,
            };
            self.offset += *group.pool_account_counts.get(self.index)? as usize;
            self.index += 1;
            if self.index == group.pool_count() {
                self.group += 1;
                self.index = 0;
            }
            return Some(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_groups_round_trip_without_intermediate_accounts() {
        let mut data = instruction();
        data.groups[0] = Group {
            direct_count: 2,
            ..Group::default()
        };
        data.groups[0].pool_account_counts[..2].copy_from_slice(&[8, 12]);
        let mut bytes = [0; MAX_PAYLOAD_LEN];
        let encoded = data.encode(&mut bytes).unwrap();
        assert_eq!(InstructionData::decode(encoded), Ok(data));
        assert_eq!(data.account_count(), 33);
        for count in [0, 1] {
            data.groups[0].direct_count = count;
            assert!(data.validate().is_err());
        }
    }

    #[test]
    fn pool_blocks_follow_the_account_layout() {
        let offsets = |data: &InstructionData| {
            let mut blocks = [(0, 0, 0); MAX_POOLS];
            let mut n = 0;
            for b in data.pool_blocks() {
                blocks[n] = (b.group, b.index, b.offset);
                n += 1;
            }
            (blocks, n)
        };
        // Fee and flashloan widen the prefix to 10; the bridged group adds 6.
        let mut data = instruction();
        let (blocks, n) = offsets(&data);
        assert_eq!(
            blocks[..n],
            [(0, 0, 16), (0, 1, 24), (0, 2, 36), (0, 3, 46)]
        );
        assert_eq!(46 + 12, data.account_count());
        // A conversion adds its quote accounts and pool; direct groups add 3.
        data.conversion_account_count = 6;
        data.group_count = 2;
        data.groups[1] = Group {
            direct_count: 2,
            ..Group::default()
        };
        data.groups[1].pool_account_counts[..2].copy_from_slice(&[7, 9]);
        let (blocks, n) = offsets(&data);
        assert_eq!(blocks[0], (0, 0, 25));
        assert_eq!(blocks[4..n], [(1, 0, 70), (1, 1, 77)]);
        assert_eq!(77 + 9, data.account_count());
    }

    fn instruction() -> InstructionData {
        let mut data = InstructionData {
            header: Header {
                minimum_profit: 42,
                compute_unit_limit: 600_000,
                no_failure: true,
                additional_fee_bp: 12,
                use_flashloan: true,
                trade_size: 123,
                constant_conversion: false,
            },
            group_count: 1,
            ..InstructionData::default()
        };
        data.groups[0] = Group {
            bridge_count: 1,
            direct_count: 2,
            ..Group::default()
        };
        data.groups[0].pool_account_counts[..4].copy_from_slice(&[8, 12, 10, 12]);
        data
    }

    #[test]
    fn compute_allowance_is_required_on_encode_and_decode() {
        for limit in [
            0,
            1,
            MAX_COMPUTE_UNIT_LIMIT,
            MAX_COMPUTE_UNIT_LIMIT + 1,
            u32::MAX,
        ] {
            let mut data = instruction();
            let mut bytes = [0; MAX_PAYLOAD_LEN];
            let len = data.encode(&mut bytes).unwrap().len();
            bytes[8..12].copy_from_slice(&limit.to_le_bytes());
            data.header.compute_unit_limit = limit;
            let valid = (1..=MAX_COMPUTE_UNIT_LIMIT).contains(&limit);
            assert_eq!(InstructionData::decode(&bytes[..len]).is_ok(), valid);
            assert_eq!(data.encode(&mut bytes).is_ok(), valid);
        }
    }

    #[test]
    fn round_trip() {
        let data = instruction();
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let bytes = data.encode(&mut buffer).unwrap();
        assert_eq!(InstructionData::decode(bytes), Ok(data));
        assert_eq!(data.account_count(), 58);
    }

    #[test]
    fn reject_truncation_trailing_flags_and_invalid_counts() {
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let len = instruction().encode(&mut buffer).unwrap().len();
        for n in 0..len {
            assert!(InstructionData::decode(&buffer[..n]).is_err());
        }
        assert!(InstructionData::decode(&buffer[..len + 1]).is_err());
        for offset in [12, 15, 24, 25, 26, 27] {
            let mut bad = buffer;
            bad[offset] = 255;
            assert!(InstructionData::decode(&bad[..len]).is_err());
        }
    }

    #[test]
    fn conversion_round_trip_and_account_bounds() {
        let mut data = instruction();
        data.conversion_account_count = 12;
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let len = data.encode(&mut buffer).unwrap().len();
        assert_eq!(buffer[24], 1);
        assert_eq!(InstructionData::decode(&buffer[..len]), Ok(data));
        assert_eq!(data.account_count(), 73);
        for n in 0..len {
            assert!(InstructionData::decode(&buffer[..n]).is_err());
        }
        buffer[26..28].copy_from_slice(&0u16.to_le_bytes());
        assert!(InstructionData::decode(&buffer[..len]).is_err());
        data.conversion_account_count = 25;
        assert!(data.validate().is_err());
        data.conversion_account_count = 12;
        data.groups[0].direct_count = 14;
        data.groups[0].pool_account_counts = [8; MAX_POOLS];
        assert!(data.validate().is_err());
    }
    #[test]
    fn idl_header_matches_the_encoding() {
        let idl: serde_json::Value = serde_json::from_str(include_str!("../idl.json")).unwrap();
        let ix = &idl["instructions"][0];
        assert_eq!(ix["discriminator"], serde_json::json!([OPCODE]));
        let mut data = instruction();
        data.conversion_account_count = 6;
        data.header.constant_conversion = true;
        let mut buffer = [0; MAX_PAYLOAD_LEN];
        let bytes = data.encode(&mut buffer).unwrap();
        let h = data.header;
        let expected = [
            ("minimum_profit", h.minimum_profit),
            ("compute_unit_limit", h.compute_unit_limit.into()),
            ("no_failure", h.no_failure.into()),
            ("additional_fee_bp", h.additional_fee_bp.into()),
            ("use_flashloan", h.use_flashloan.into()),
            ("trade_size", h.trade_size),
            ("flags", 3),
            ("group_count", data.group_count.into()),
        ];
        let args = ix["args"].as_array().unwrap();
        assert_eq!(args.len(), expected.len());
        let mut offset = 0;
        for (arg, (name, value)) in args.iter().zip(expected) {
            let size = match arg["type"].as_str().unwrap() {
                "u64" => 8,
                "u32" => 4,
                "u16" => 2,
                "u8" | "bool" => 1,
                other => panic!("unexpected IDL arg type {other}"),
            };
            let mut le = [0; 8];
            le[..size].copy_from_slice(&bytes[offset..offset + size]);
            assert_eq!(arg["name"], name);
            assert_eq!(u64::from_le_bytes(le), value, "{name}");
            offset += size;
        }
        assert_eq!(offset, PREFIX_LEN + 1);
    }
    #[test]
    fn constant_conversion_flag_is_opt_in_and_requires_converter() {
        let mut data = instruction();
        data.header.constant_conversion = true;
        assert!(data.validate().is_err());
        data.conversion_account_count = 12;
        let mut bytes = [0; MAX_PAYLOAD_LEN];
        let len = data.encode(&mut bytes).unwrap().len();
        assert_eq!(bytes[24], 3);
        assert_eq!(InstructionData::decode(&bytes[..len]), Ok(data));
        bytes[24] = 2;
        assert!(InstructionData::decode(&bytes[..len]).is_err());
        bytes[24] = 7;
        assert!(InstructionData::decode(&bytes[..len]).is_err());
    }
}
