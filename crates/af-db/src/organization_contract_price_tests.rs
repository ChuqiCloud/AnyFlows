#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    #[test]
    fn decimal_contract_prices_keep_fixed_point_values() {
        let value =
            Decimal::from_str_exact("0.0000000000000000000000000001").expect("定点价格应可解析");
        assert_eq!(value.scale(), 28);
        assert_eq!(value.to_string(), "0.0000000000000000000000000001");
    }
}
