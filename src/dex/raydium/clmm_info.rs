use anyhow::Result;
use solana_program::pubkey::Pubkey;

pub const TICK_ARRAY_SEED: &str = "tick_array";
pub const TICK_ARRAY_SIZE: i32 = 60;
pub const TICK_ARRAY_SIZE_USIZE: usize = 60;
pub const REWARD_NUM: usize = 3;
pub const MIN_TICK: i32 = -443_636;
pub const MAX_TICK: i32 = -MIN_TICK;
pub const TICK_ARRAY_BITMAP_SIZE: i32 = 512;
pub const EXTENSION_TICKARRAY_BITMAP_SIZE: usize = 14;

pub const POOL_TICK_ARRAY_BITMAP_SEED: &str = "pool_tick_array_bitmap_extension";

pub enum RewardState {
    Uninitialized,
    Initialized,
    Opening,
    Ended,
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct RewardInfo {
    pub reward_state: u8,
    pub open_time: u64,
    pub end_time: u64,
    pub last_update_time: u64,
    pub emissions_per_second_x64: u128,
    pub reward_total_emissioned: u64,
    pub reward_claimed: u64,
    pub token_mint: Pubkey,
    pub token_vault: Pubkey,
    pub authority: Pubkey,
    pub reward_growth_global_x64: u128,
}

impl RewardInfo {
    pub fn new(authority: Pubkey) -> Self {
        Self {
            authority,
            ..Default::default()
        }
    }

    pub fn initialized(&self) -> bool {
        self.token_mint.ne(&Pubkey::default())
    }
}

#[derive(Default, Debug)]
pub struct PoolState {
    pub bump: [u8; 1],
    pub amm_config: Pubkey,
    pub owner: Pubkey,

    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,

    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,

    pub observation_key: Pubkey,

    pub mint_decimals_0: u8,
    pub mint_decimals_1: u8,

    pub tick_spacing: u16,
    pub liquidity: u128,
    pub sqrt_price_x64: u128,
    pub tick_current: i32,

    pub padding3: u16,
    pub padding4: u16,

    pub fee_growth_global_0_x64: u128,
    pub fee_growth_global_1_x64: u128,

    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    pub swap_in_amount_token_0: u128,
    pub swap_out_amount_token_1: u128,
    pub swap_in_amount_token_1: u128,
    pub swap_out_amount_token_0: u128,

    pub status: u8,
    pub padding: [u8; 7],

    pub reward_infos: [RewardInfo; REWARD_NUM],

    pub tick_array_bitmap: [u64; 16],

    pub total_fees_token_0: u64,
    pub total_fees_claimed_token_0: u64,
    pub total_fees_token_1: u64,
    pub total_fees_claimed_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    pub open_time: u64,
    pub recent_epoch: u64,

    pub padding1: [u64; 24],
    pub padding2: [u64; 32],
}

#[derive(Debug, Clone)]
pub struct TickArrayBitmapExtensionState {
    pub pool_id: Pubkey,
    pub positive_tick_array_bitmap: [[u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
    pub negative_tick_array_bitmap: [[u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE],
}

impl PoolState {
    pub fn load_checked(data: &[u8]) -> Result<Self> {
        const TICK_ARRAY_BITMAP_OFFSET: usize = 896;
        const TICK_ARRAY_BITMAP_BYTES: usize = 16 * 8;
        if data.len() < 8 + TICK_ARRAY_BITMAP_OFFSET + TICK_ARRAY_BITMAP_BYTES {
            return Err(anyhow::anyhow!(
                "Invalid data length for RaydiumClmmPoolState"
            ));
        }

        let data = &data[8..]; // Skip the discriminator
        let mut offset = 0;

        offset += 1;

        let mut amm_config = [0u8; 32];
        amm_config.copy_from_slice(&data[offset..offset + 32]);
        let amm_config = Pubkey::new_from_array(amm_config);
        offset += 32;

        offset += 32;

        let mut token_mint_0 = [0u8; 32];
        token_mint_0.copy_from_slice(&data[offset..offset + 32]);
        let token_mint_0 = Pubkey::new_from_array(token_mint_0);
        offset += 32;

        let mut token_mint_1 = [0u8; 32];
        token_mint_1.copy_from_slice(&data[offset..offset + 32]);
        let token_mint_1 = Pubkey::new_from_array(token_mint_1);
        offset += 32;

        let mut token_vault_0 = [0u8; 32];
        token_vault_0.copy_from_slice(&data[offset..offset + 32]);
        let token_vault_0 = Pubkey::new_from_array(token_vault_0);
        offset += 32;

        let mut token_vault_1 = [0u8; 32];
        token_vault_1.copy_from_slice(&data[offset..offset + 32]);
        let token_vault_1 = Pubkey::new_from_array(token_vault_1);
        offset += 32;

        let mut observation_key = [0u8; 32];
        observation_key.copy_from_slice(&data[offset..offset + 32]);
        let observation_key = Pubkey::new_from_array(observation_key);
        offset += 32;

        offset += 2;

        let mut tick_spacing_bytes = [0u8; 2];
        tick_spacing_bytes.copy_from_slice(&data[offset..offset + 2]);
        let tick_spacing = u16::from_le_bytes(tick_spacing_bytes);
        offset += 2;

        offset += 16;

        // Skip sqrt_price_x64
        offset += 16;

        let mut tick_current_bytes = [0u8; 4];
        tick_current_bytes.copy_from_slice(&data[offset..offset + 4]);
        let tick_current = i32::from_le_bytes(tick_current_bytes);
        let _ = offset + 4;

        let mut tick_array_bitmap = [0u64; 16];
        for (i, chunk) in tick_array_bitmap.iter_mut().enumerate() {
            let start = TICK_ARRAY_BITMAP_OFFSET + i * 8;
            let end = start + 8;
            let mut bits = [0u8; 8];
            bits.copy_from_slice(&data[start..end]);
            *chunk = u64::from_le_bytes(bits);
        }

        Ok(Self {
            amm_config,
            token_mint_0,
            token_mint_1,
            token_vault_0,
            token_vault_1,
            observation_key,
            tick_spacing,
            tick_current,
            tick_array_bitmap,
            ..Default::default()
        })
    }
}

pub fn parse_bitmap_extension(data: &[u8]) -> Option<TickArrayBitmapExtensionState> {
    // 8 discriminator + 32 pool + (14 * 8 * 8 * 2) bitmap bytes
    const HEADER_BYTES: usize = 8 + 32;
    const BITMAP_BYTES: usize = EXTENSION_TICKARRAY_BITMAP_SIZE * 8 * 8;
    const TOTAL_BYTES: usize = HEADER_BYTES + BITMAP_BYTES * 2;

    if data.len() < TOTAL_BYTES {
        return None;
    }

    let mut pool_id = [0u8; 32];
    pool_id.copy_from_slice(&data[8..40]);
    let pool_id = Pubkey::new_from_array(pool_id);

    let mut cursor = 40;
    let mut positive_tick_array_bitmap = [[0u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];
    for row in &mut positive_tick_array_bitmap {
        for word in row.iter_mut() {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&data[cursor..cursor + 8]);
            *word = u64::from_le_bytes(bytes);
            cursor += 8;
        }
    }

    let mut negative_tick_array_bitmap = [[0u64; 8]; EXTENSION_TICKARRAY_BITMAP_SIZE];
    for row in &mut negative_tick_array_bitmap {
        for word in row.iter_mut() {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&data[cursor..cursor + 8]);
            *word = u64::from_le_bytes(bytes);
            cursor += 8;
        }
    }

    Some(TickArrayBitmapExtensionState {
        pool_id,
        positive_tick_array_bitmap,
        negative_tick_array_bitmap,
    })
}

pub fn compute_tick_array_start_index(tick: i32, tick_spacing: u16) -> i32 {
    let ticks_in_array = TICK_ARRAY_SIZE * tick_spacing as i32;
    let mut start = tick / ticks_in_array;
    if tick < 0 && tick % ticks_in_array != 0 {
        start = start - 1
    }
    start * ticks_in_array
}

pub fn get_tick_array_pubkeys(
    pool_pubkey: &Pubkey,
    tick_current: i32,
    tick_spacing: u16,
    offsets: &[i32],
    raydium_clmm_program_id: &Pubkey,
) -> Result<Vec<Pubkey>> {
    let mut result = Vec::with_capacity(offsets.len());
    let ticks_in_array = TICK_ARRAY_SIZE * tick_spacing as i32;

    for &offset in offsets {
        let base_start_index = compute_tick_array_start_index(tick_current, tick_spacing);

        let offset_start_index = base_start_index + offset * ticks_in_array;

        let seeds = &[
            TICK_ARRAY_SEED.as_bytes(),
            pool_pubkey.as_ref(),
            &offset_start_index.to_be_bytes(),
        ];

        let (pubkey, _) = Pubkey::find_program_address(seeds, raydium_clmm_program_id);
        result.push(pubkey);
    }

    Ok(result)
}

fn tick_count(tick_spacing: u16) -> i32 {
    TICK_ARRAY_SIZE * i32::from(tick_spacing)
}

fn min_start_index(tick_spacing: u16) -> i32 {
    compute_tick_array_start_index(MIN_TICK, tick_spacing)
}

fn max_start_index(tick_spacing: u16) -> i32 {
    compute_tick_array_start_index(MAX_TICK, tick_spacing)
}

fn bit_is_set(words: &[u64], bit_index: usize) -> bool {
    let word_index = bit_index / 64;
    let bit_offset = bit_index % 64;
    words
        .get(word_index)
        .map(|word| (word & (1u64 << bit_offset)) != 0)
        .unwrap_or(false)
}

fn tick_array_offset_in_bitmap(start_index: i32, tick_spacing: u16) -> Option<i32> {
    let ticks_in_one_bitmap = tick_count(tick_spacing) * TICK_ARRAY_BITMAP_SIZE;
    if ticks_in_one_bitmap <= 0 {
        return None;
    }
    let m = start_index.abs() % ticks_in_one_bitmap;
    let mut offset = m / tick_count(tick_spacing);
    if start_index < 0 && m != 0 {
        offset = TICK_ARRAY_BITMAP_SIZE - offset;
    }
    if (0..TICK_ARRAY_BITMAP_SIZE).contains(&offset) {
        Some(offset)
    } else {
        None
    }
}

fn extension_bitmap_offset(start_index: i32, tick_spacing: u16) -> Option<usize> {
    let ticks_in_one_bitmap = tick_count(tick_spacing) * TICK_ARRAY_BITMAP_SIZE;
    if ticks_in_one_bitmap <= 0 {
        return None;
    }

    // Extension bitmap only represents ranges outside the default bitmap.
    if start_index >= -ticks_in_one_bitmap && start_index < ticks_in_one_bitmap {
        return None;
    }

    let abs_index = start_index.abs();
    let mut offset = abs_index / ticks_in_one_bitmap - 1;
    if start_index < 0 && abs_index % ticks_in_one_bitmap == 0 {
        offset -= 1;
    }
    if offset < 0 || offset as usize >= EXTENSION_TICKARRAY_BITMAP_SIZE {
        return None;
    }
    Some(offset as usize)
}

fn is_initialized_in_default_bitmap(
    start_index: i32,
    tick_spacing: u16,
    tick_array_bitmap: &[u64; 16],
) -> Option<bool> {
    let ticks_per_array = tick_count(tick_spacing);
    let tick_boundary = ticks_per_array * TICK_ARRAY_BITMAP_SIZE;
    if ticks_per_array <= 0 || start_index < -tick_boundary || start_index >= tick_boundary {
        return None;
    }

    let mut compressed = start_index / ticks_per_array + TICK_ARRAY_BITMAP_SIZE;
    if start_index < 0 && start_index % ticks_per_array != 0 {
        compressed -= 1;
    }
    if !(0..(TICK_ARRAY_BITMAP_SIZE * 2)).contains(&compressed) {
        return None;
    }

    Some(bit_is_set(tick_array_bitmap, compressed as usize))
}

fn is_initialized_in_extension_bitmap(
    start_index: i32,
    tick_spacing: u16,
    extension: &TickArrayBitmapExtensionState,
) -> Option<bool> {
    let offset = extension_bitmap_offset(start_index, tick_spacing)?;
    let bit = tick_array_offset_in_bitmap(start_index, tick_spacing)? as usize;
    let words = if start_index < 0 {
        extension.negative_tick_array_bitmap[offset]
    } else {
        extension.positive_tick_array_bitmap[offset]
    };
    Some(bit_is_set(&words, bit))
}

fn is_initialized_tick_array(
    start_index: i32,
    tick_spacing: u16,
    tick_array_bitmap: &[u64; 16],
    extension: Option<&TickArrayBitmapExtensionState>,
) -> bool {
    let ticks_per_array = tick_count(tick_spacing);
    if ticks_per_array <= 0
        || start_index % ticks_per_array != 0
        || start_index < min_start_index(tick_spacing)
        || start_index > max_start_index(tick_spacing)
    {
        return false;
    }

    if let Some(in_default) =
        is_initialized_in_default_bitmap(start_index, tick_spacing, tick_array_bitmap)
    {
        return in_default;
    }

    extension
        .and_then(|ext| is_initialized_in_extension_bitmap(start_index, tick_spacing, ext))
        .unwrap_or(false)
}

fn find_initialized_start(
    from_start: i32,
    tick_spacing: u16,
    tick_array_bitmap: &[u64; 16],
    extension: Option<&TickArrayBitmapExtensionState>,
    zero_for_one: bool,
    include_current: bool,
) -> Option<i32> {
    let ticks_per_array = tick_count(tick_spacing);
    if ticks_per_array <= 0 {
        return None;
    }

    if include_current
        && is_initialized_tick_array(from_start, tick_spacing, tick_array_bitmap, extension)
    {
        return Some(from_start);
    }

    let min_start = min_start_index(tick_spacing);
    let max_start = max_start_index(tick_spacing);
    let mut current = from_start;
    loop {
        current = if zero_for_one {
            current - ticks_per_array
        } else {
            current + ticks_per_array
        };
        if current < min_start || current > max_start {
            return None;
        }
        if is_initialized_tick_array(current, tick_spacing, tick_array_bitmap, extension) {
            return Some(current);
        }
    }
}

fn select_tick_array_starts(current_start: i32, mut candidates: Vec<i32>) -> Vec<i32> {
    if candidates.is_empty() {
        return Vec::new();
    }

    candidates.sort();
    candidates.dedup();

    candidates.sort_by_key(|start| {
        let delta = i64::from(*start) - i64::from(current_start);
        (delta.abs(), i64::from(*start))
    });

    let mut selected: Vec<i32> = candidates.into_iter().take(3).collect();
    selected.sort();
    selected.dedup();
    selected
}

fn derive_tick_array_pubkeys(
    pool_pubkey: &Pubkey,
    program_id: &Pubkey,
    starts: &[i32],
) -> Vec<Pubkey> {
    starts
        .iter()
        .map(|start| {
            let seeds = &[
                TICK_ARRAY_SEED.as_bytes(),
                pool_pubkey.as_ref(),
                &start.to_be_bytes(),
            ];
            Pubkey::find_program_address(seeds, program_id).0
        })
        .collect()
}

pub fn get_initialized_tick_array_pubkeys(
    pool_pubkey: &Pubkey,
    pool_state: &PoolState,
    extension: Option<&TickArrayBitmapExtensionState>,
    program_id: &Pubkey,
) -> Result<Vec<Pubkey>> {
    if pool_state.tick_spacing == 0 {
        return Err(anyhow::anyhow!("tick_spacing is zero for pool {}", pool_pubkey));
    }

    if let Some(ext) = extension {
        // Ignore extension accounts that don't belong to this pool.
        if ext.pool_id != *pool_pubkey {
            return Err(anyhow::anyhow!(
                "tick bitmap extension pool mismatch for {}",
                pool_pubkey
            ));
        }
    }

    let current_start =
        compute_tick_array_start_index(pool_state.tick_current, pool_state.tick_spacing);

    let first_down = find_initialized_start(
        current_start,
        pool_state.tick_spacing,
        &pool_state.tick_array_bitmap,
        extension,
        true,
        true,
    );
    let first_up = find_initialized_start(
        current_start,
        pool_state.tick_spacing,
        &pool_state.tick_array_bitmap,
        extension,
        false,
        true,
    );

    let mut candidates = Vec::new();
    if let Some(start) = first_down {
        candidates.push(start);
    }
    if let Some(start) = first_up {
        candidates.push(start);
    }

    let mut last_down = first_down.unwrap_or(current_start);
    let mut last_up = first_up.unwrap_or(current_start);

    while candidates.len() < 3 {
        let mut progressed = false;

        if let Some(next_down) = find_initialized_start(
            last_down,
            pool_state.tick_spacing,
            &pool_state.tick_array_bitmap,
            extension,
            true,
            false,
        ) {
            if !candidates.contains(&next_down) {
                candidates.push(next_down);
            }
            last_down = next_down;
            progressed = true;
        }

        if candidates.len() >= 3 {
            break;
        }

        if let Some(next_up) = find_initialized_start(
            last_up,
            pool_state.tick_spacing,
            &pool_state.tick_array_bitmap,
            extension,
            false,
            false,
        ) {
            if !candidates.contains(&next_up) {
                candidates.push(next_up);
            }
            last_up = next_up;
            progressed = true;
        }

        if !progressed {
            break;
        }
    }

    let starts = select_tick_array_starts(current_start, candidates);
    if starts.is_empty() {
        return Err(anyhow::anyhow!(
            "no initialized CLMM tick arrays found (current_start={})",
            current_start
        ));
    }

    Ok(derive_tick_array_pubkeys(pool_pubkey, program_id, &starts))
}
