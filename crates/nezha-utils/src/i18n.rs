use std::collections::HashMap;
use std::sync::OnceLock;

static TRANSLATIONS: OnceLock<HashMap<String, HashMap<String, String>>> = OnceLock::new();
static CURRENT_LANG: OnceLock<String> = OnceLock::new();

/// i18n 本地化器
pub struct Localizer;

impl Localizer {
    /// 翻译文本
    pub fn t(key: &str) -> String {
        let lang = CURRENT_LANG.get().map(|s| s.as_str()).unwrap_or("en_US");
        let translations = TRANSLATIONS.get();

        if let Some(trans) = translations {
            if let Some(lang_map) = trans.get(lang) {
                if let Some(value) = lang_map.get(key) {
                    return value.clone();
                }
            }
        }
        key.to_string()
    }

    /// 翻译文本（带格式化参数）
    pub fn tf(key: &str, args: &[&str]) -> String {
        let translated = Self::t(key);
        let mut result = translated;
        for (i, arg) in args.iter().enumerate() {
            result = result.replacen(&format!("%{}", i + 1), arg, 1);
        }
        // 也支持 %s 风格
        for arg in args {
            if let Some(pos) = result.find("%s") {
                result.replace_range(pos..pos + 2, arg);
            }
        }
        result
    }

    /// 翻译错误消息
    pub fn error_t(key: &str) -> String {
        Self::t(key)
    }
}

/// 初始化本地化系统
pub fn init_i18n(lang: &str) {
    let _ = CURRENT_LANG.set(lang.to_string());

    let mut translations = HashMap::new();

    // 英文翻译
    let mut en_us = HashMap::new();
    en_us.insert("IP Changed".to_string(), "IP Changed".to_string());
    en_us.insert(
        "Scheduled Task Executed Successfully".to_string(),
        "Scheduled Task Executed Successfully".to_string(),
    );
    en_us.insert(
        "Scheduled Task Executed Failed".to_string(),
        "Scheduled Task Executed Failed".to_string(),
    );
    en_us.insert("No Data".to_string(), "No Data".to_string());
    en_us.insert("Good".to_string(), "Good".to_string());
    en_us.insert(
        "Low Availability".to_string(),
        "Low Availability".to_string(),
    );
    en_us.insert("Down".to_string(), "Down".to_string());
    en_us.insert("unauthorized".to_string(), "Unauthorized".to_string());
    en_us.insert(
        "permission denied".to_string(),
        "Permission Denied".to_string(),
    );
    en_us.insert(
        "database error".to_string(),
        "Database Error".to_string(),
    );
    translations.insert("en_US".to_string(), en_us);

    // 中文翻译
    let mut zh_cn = HashMap::new();
    zh_cn.insert("IP Changed".to_string(), "IP变更".to_string());
    zh_cn.insert(
        "Scheduled Task Executed Successfully".to_string(),
        "计划任务执行成功".to_string(),
    );
    zh_cn.insert(
        "Scheduled Task Executed Failed".to_string(),
        "计划任务执行失败".to_string(),
    );
    zh_cn.insert("No Data".to_string(), "无数据".to_string());
    zh_cn.insert("Good".to_string(), "正常".to_string());
    zh_cn.insert("Low Availability".to_string(), "低可用".to_string());
    zh_cn.insert("Down".to_string(), "故障".to_string());
    zh_cn.insert("unauthorized".to_string(), "未授权".to_string());
    zh_cn.insert("permission denied".to_string(), "权限不足".to_string());
    zh_cn.insert("database error".to_string(), "数据库错误".to_string());
    translations.insert("zh_CN".to_string(), zh_cn);

    let _ = TRANSLATIONS.set(translations);
}
