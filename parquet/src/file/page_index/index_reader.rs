// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Support for reading [`Index`] and [`OffsetIndex`] from parquet metadata.

use crate::basic::Type;
use crate::data_type::Int96;
use crate::errors::ParquetError;
use crate::file::metadata::ColumnChunkMetaData;
use crate::file::page_index::index::{Index, NativeIndex};
use crate::file::page_index::offset_index::OffsetIndexMetaData;
use crate::file::reader::ChunkReader;
use crate::format::{ColumnIndex, OffsetIndex};
use crate::thrift::{TCompactSliceInputProtocol, TSerializable};
use std::ops::Range;

/// Computes the covering range of two optional ranges
///
/// For example `acc_range(Some(7..9), Some(1..3)) = Some(1..9)`
pub(crate) fn acc_range(a: Option<Range<u64>>, b: Option<Range<u64>>) -> Option<Range<u64>> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.start.min(b.start)..a.end.max(b.end)),
        (None, x) | (x, None) => x,
    }
}

/// Reads per-column [`Index`] for all columns of a row group by
/// decoding [`ColumnIndex`] .
///
/// Returns a vector of `index[column_number]`.
///
/// Returns `None` if this row group does not contain a [`ColumnIndex`].
///
/// See [Page Index Documentation] for more details.
///
/// [Page Index Documentation]: https://github.com/apache/parquet-format/blob/master/PageIndex.md
#[deprecated(
    since = "55.2.0",
    note = "Use ParquetMetaDataReader instead; will be removed in 58.0.0"
)]
pub fn read_columns_indexes<R: ChunkReader>(
    reader: &R,
    chunks: &[ColumnChunkMetaData],
) -> Result<Option<Vec<Index>>, ParquetError> {
    let fetch = chunks
        .iter()
        .fold(None, |range, c| acc_range(range, c.column_index_range()));

    let fetch = match fetch {
        Some(r) => r,
        None => return Ok(None),
    };

    let bytes = reader.get_bytes(fetch.start as _, (fetch.end - fetch.start).try_into()?)?;

    Some(
        chunks
            .iter()
            .map(|c| match c.column_index_range() {
                Some(r) => decode_column_index(
                    &bytes[usize::try_from(r.start - fetch.start)?
                        ..usize::try_from(r.end - fetch.start)?],
                    c.column_type(),
                ),
                None => Ok(Index::NONE),
            })
            .collect(),
    )
    .transpose()
}

/// Reads per-column [`OffsetIndexMetaData`] for all columns of a row group by
/// decoding [`OffsetIndex`] .
///
/// Returns a vector of `offset_index[column_number]`.
///
/// Returns `None` if this row group does not contain an [`OffsetIndex`].
///
/// See [Page Index Documentation] for more details.
///
/// [Page Index Documentation]: https://github.com/apache/parquet-format/blob/master/PageIndex.md
#[deprecated(
    since = "55.2.0",
    note = "Use ParquetMetaDataReader instead; will be removed in 58.0.0"
)]
pub fn read_offset_indexes<R: ChunkReader>(
    reader: &R,
    chunks: &[ColumnChunkMetaData],
) -> Result<Option<Vec<OffsetIndexMetaData>>, ParquetError> {
    let fetch = chunks
        .iter()
        .fold(None, |range, c| acc_range(range, c.offset_index_range()));

    let fetch = match fetch {
        Some(r) => r,
        None => return Ok(None),
    };

    let bytes = reader.get_bytes(fetch.start as _, (fetch.end - fetch.start).try_into()?)?;

    Some(
        chunks
            .iter()
            .map(|c| match c.offset_index_range() {
                Some(r) => decode_offset_index(
                    &bytes[usize::try_from(r.start - fetch.start)?
                        ..usize::try_from(r.end - fetch.start)?],
                ),
                None => Err(general_err!("missing offset index")),
            })
            .collect(),
    )
    .transpose()
}

pub(crate) fn decode_offset_index(data: &[u8]) -> Result<OffsetIndexMetaData, ParquetError> {
    let mut prot = TCompactSliceInputProtocol::new(data);
    let offset = OffsetIndex::read_from_in_protocol(&mut prot)?;
    OffsetIndexMetaData::try_new(offset)
}

pub(crate) fn decode_column_index(data: &[u8], column_type: Type) -> Result<Index, ParquetError> {
    let mut prot = TCompactSliceInputProtocol::new(data);

    let index = ColumnIndex::read_from_in_protocol(&mut prot)?;

    let index = match column_type {
        Type::BOOLEAN => Index::BOOLEAN(NativeIndex::<bool>::try_new(index)?),
        Type::INT32 => Index::INT32(NativeIndex::<i32>::try_new(index)?),
        Type::INT64 => Index::INT64(NativeIndex::<i64>::try_new(index)?),
        Type::INT96 => Index::INT96(NativeIndex::<Int96>::try_new(index)?),
        Type::FLOAT => Index::FLOAT(NativeIndex::<f32>::try_new(index)?),
        Type::DOUBLE => Index::DOUBLE(NativeIndex::<f64>::try_new(index)?),
        Type::BYTE_ARRAY => Index::BYTE_ARRAY(NativeIndex::try_new(index)?),
        Type::FIXED_LEN_BYTE_ARRAY => Index::FIXED_LEN_BYTE_ARRAY(NativeIndex::try_new(index)?),
    };

    Ok(index)
}
