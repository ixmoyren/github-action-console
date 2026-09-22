pub const APP_TITLE: &str = "GitHub Action Console";
pub const LABEL_VERSION: &str = "版本";
pub const LABEL_BUILD_TARGET: &str = "构建目标";
pub const LABEL_PACKAGING_CONFIG: &str = "打包配置";
pub const UNSPECIFIED: &str = "未指定";

pub const LOGIN_TITLE: &str = "登录 GitHub";
pub const LOGIN_START: &str = "使用 GitHub 登录";
pub const LOGIN_STARTING: &str = "正在请求授权…";
pub const LOGIN_WAITING: &str = "等待授权中…";
pub const LOGIN_INSTRUCTION: &str = "请在浏览器中打开下面的地址，并输入一次性代码：";
pub const LOGIN_USER_CODE: &str = "一次性代码";
pub const LOGIN_COPY: &str = "复制代码";
pub const LOGIN_COPIED: &str = "已复制";
pub const LOGIN_OPEN_BROWSER: &str = "打开验证地址";
pub const LOGIN_EXPIRES_IN: &str = "有效期（秒）";
pub const LOGIN_LOGGED_IN_AS: &str = "当前登录用户";
pub const LOGIN_SIGN_OUT: &str = "登出";

pub const NOTICE_DEVICE_FLOW_UNAVAILABLE: &str =
    "未配置 OAuth App 的 client_id，无法使用 Device Flow 登录。";
pub const NOTICE_EXPIRED: &str = "授权码已过期，请重新登录。";
pub const NOTICE_DENIED: &str = "授权已被拒绝。";
pub const NOTICE_INVALID_CREDENTIALS: &str = "登录凭据已失效，请重新登录。";
pub const NOTICE_NETWORK: &str = "网络请求失败，请检查网络后重试。";
pub const NOTICE_UNEXPECTED: &str = "出现未预期的错误，请重试。";
