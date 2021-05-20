// coincheckの仕様に合わせて加工する
pub fn to_request_string(v: f64) -> String {
    // 小数以外で5桁以上あるなら小数点以下は省く
    let s = format!("{:.0}", v);
    if s.chars().count() >= 5 {
        return s.to_owned();
    }

    // 小数含めて全体で5桁（ドット含めると6文字）にする
    let s = format!("{:.5}", v);
    if s.chars().count() < 6 {
        return s.to_owned();
    }
    s[0..6].to_owned()
}
