use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Number(f64),
    Text(String),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Column {
    Numeric(Vec<Option<f64>>),
    Text(Vec<Option<String>>),
}

impl Column {
    pub fn len(&self) -> usize {
        match self {
            Self::Numeric(values) => values.len(),
            Self::Text(values) => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn value(&self, index: usize) -> Option<DataValue> {
        match self {
            Self::Numeric(values) => values.get(index).map(|value| {
                value
                    .filter(|value| value.is_finite())
                    .map(DataValue::Number)
                    .unwrap_or(DataValue::Missing)
            }),
            Self::Text(values) => values.get(index).map(|value| {
                value
                    .as_ref()
                    .map(|value| DataValue::Text(value.clone()))
                    .unwrap_or(DataValue::Missing)
            }),
        }
    }

    fn append(&mut self, other: &Column) -> Result<(), DataError> {
        match (self, other) {
            (Self::Numeric(target), Self::Numeric(values)) => target.extend(values.iter().copied()),
            (Self::Text(target), Self::Text(values)) => target.extend(values.iter().cloned()),
            _ => return Err(DataError::ColumnTypeMismatch),
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DataError {
    #[error("a data table requires at least one column")]
    Empty,
    #[error("column names must be non-empty and unique")]
    InvalidColumnName,
    #[error("all columns must have the same length")]
    LengthMismatch,
    #[error("column '{0}' does not exist")]
    MissingColumn(String),
    #[error("column has the wrong type")]
    ColumnTypeMismatch,
    #[error("appended data must contain the same columns")]
    SchemaMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataTable {
    columns: BTreeMap<String, Column>,
    len: usize,
}

impl DataTable {
    pub fn new(columns: impl IntoIterator<Item = (String, Column)>) -> Result<Self, DataError> {
        let mut result = BTreeMap::new();
        for (name, column) in columns {
            if name.trim().is_empty() || result.contains_key(&name) {
                return Err(DataError::InvalidColumnName);
            }
            result.insert(name, column);
        }
        let Some(len) = result.values().next().map(Column::len) else {
            return Err(DataError::Empty);
        };
        if result.values().any(|column| column.len() != len) {
            return Err(DataError::LengthMismatch);
        }
        Ok(Self {
            columns: result,
            len,
        })
    }

    pub fn numeric(
        columns: impl IntoIterator<Item = (String, Vec<f64>)>,
    ) -> Result<Self, DataError> {
        Self::new(columns.into_iter().map(|(name, values)| {
            (
                name,
                Column::Numeric(
                    values
                        .into_iter()
                        .map(|value| value.is_finite().then_some(value))
                        .collect(),
                ),
            )
        }))
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn columns(&self) -> impl Iterator<Item = (&str, &Column)> {
        self.columns
            .iter()
            .map(|(name, column)| (name.as_str(), column))
    }

    pub fn column(&self, name: &str) -> Result<&Column, DataError> {
        self.columns
            .get(name)
            .ok_or_else(|| DataError::MissingColumn(name.to_owned()))
    }

    pub fn numeric_column(&self, name: &str) -> Result<&[Option<f64>], DataError> {
        match self.column(name)? {
            Column::Numeric(values) => Ok(values),
            Column::Text(_) => Err(DataError::ColumnTypeMismatch),
        }
    }

    pub fn text_column(&self, name: &str) -> Result<&[Option<String>], DataError> {
        match self.column(name)? {
            Column::Text(values) => Ok(values),
            Column::Numeric(_) => Err(DataError::ColumnTypeMismatch),
        }
    }

    pub fn value(&self, row: usize, column: &str) -> Result<Option<DataValue>, DataError> {
        Ok(self.column(column)?.value(row))
    }

    pub fn append(&mut self, other: &DataTable) -> Result<(), DataError> {
        if self.columns.keys().ne(other.columns.keys()) {
            return Err(DataError::SchemaMismatch);
        }
        for (name, column) in &mut self.columns {
            column.append(&other.columns[name])?;
        }
        self.len += other.len;
        Ok(())
    }
}

/// Shared tabular data with an observable monotonically increasing version.
/// Visualization systems can skip regeneration when the version is unchanged.
#[derive(Debug, Clone)]
pub struct DataSource {
    table: Arc<RwLock<DataTable>>,
    version: Arc<AtomicU64>,
}

impl DataSource {
    pub fn new(table: DataTable) -> Self {
        Self {
            table: Arc::new(RwLock::new(table)),
            version: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn snapshot(&self) -> DataTable {
        self.table.read().expect("data source poisoned").clone()
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub fn replace(&self, table: DataTable) {
        *self.table.write().expect("data source poisoned") = table;
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    pub fn append(&self, table: &DataTable) -> Result<(), DataError> {
        self.table
            .write()
            .expect("data source poisoned")
            .append(table)?;
        self.version.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_enforce_rectangular_schema() {
        let result = DataTable::new([
            ("x".to_owned(), Column::Numeric(vec![Some(1.0)])),
            ("y".to_owned(), Column::Numeric(vec![Some(2.0), Some(3.0)])),
        ]);
        assert_eq!(result, Err(DataError::LengthMismatch));
    }

    #[test]
    fn data_source_versions_replace_and_append() {
        let source = DataSource::new(DataTable::numeric([("x".to_owned(), vec![1.0])]).unwrap());
        source
            .append(&DataTable::numeric([("x".to_owned(), vec![2.0])]).unwrap())
            .unwrap();
        assert_eq!(source.version(), 1);
        assert_eq!(source.snapshot().len(), 2);
        source.replace(DataTable::numeric([("x".to_owned(), vec![5.0])]).unwrap());
        assert_eq!(source.version(), 2);
    }
}
