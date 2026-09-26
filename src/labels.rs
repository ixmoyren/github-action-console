pub const APP_TITLE: &str = "GitHub Action Console";
pub const LABEL_VERSION: &str = "版本";
pub const LABEL_BUILD_TARGET: &str = "构建目标";
pub const LABEL_PACKAGING_CONFIG: &str = "打包配置";
pub const UNSPECIFIED: &str = "未指定";

pub const LOGIN_TITLE: &str = "使用 OAuth 登录";
pub const LOGIN_START: &str = "使用 OAuth 登录";
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
pub const NOTICE_MISSING_CLIENT_ID: &str = "请输入 Client ID";
pub const NOTICE_EXPIRED: &str = "授权码已过期，请重新登录。";
pub const NOTICE_DENIED: &str = "授权已被拒绝。";
pub const NOTICE_INVALID_CREDENTIALS: &str = "登录凭据已失效，请重新登录。";
pub const NOTICE_NETWORK: &str = "网络请求失败，请检查网络后重试。";
pub const NOTICE_UNEXPECTED: &str = "出现未预期的错误，请重试。";
pub const NOTICE_MISSING_SCOPES: &str = "凭据权限不足。需要 repo + workflow（粗粒度 PAT），或 Contents: RW、Actions: RW、Workflows: RW（细粒度 PAT）。";

pub const LOGIN_CLIENT_ID_PLACEHOLDER: &str = "请输入 OAuth Client ID";
pub const LOGIN_PAT_BUTTON: &str = "使用 Personal Access Token 登录";
pub const LOGIN_PAT_SUBMIT_ARROW: &str = "→";
pub const LOGIN_PAT_BACK: &str = "←";
pub const LOGIN_PAT_PLACEHOLDER: &str = "请输入 Personal Access Token";
pub const LOGIN_VALIDATING: &str = "正在验证凭据…";

pub const REPOSITORIES_TITLE: &str = "仓库";
pub const REPOSITORIES_SEARCH_PLACEHOLDER: &str = "按名称搜索仓库";
pub const REPOSITORIES_SORT_UPDATED: &str = "按最近更新";
pub const REPOSITORIES_SORT_PUSHED: &str = "按最近推送";
pub const REPOSITORIES_LOAD_MORE: &str = "加载更多";
pub const REPOSITORIES_REFRESH: &str = "刷新";
pub const REPOSITORIES_LOADING: &str = "正在加载仓库…";
pub const REPOSITORIES_EMPTY: &str = "没有匹配的仓库。";
pub const REPOSITORIES_PRIVATE: &str = "私有";
pub const REPOSITORIES_PUBLIC: &str = "公开";
pub const REPOSITORIES_COLUMN_NAME: &str = "仓库";
pub const REPOSITORIES_COLUMN_VISIBILITY: &str = "可见性";
pub const REPOSITORIES_COLUMN_BRANCH: &str = "默认分支";
pub const REPOSITORIES_COLUMN_COMMIT: &str = "最近提交";
pub const REPOSITORIES_COLUMN_COMMIT_DATE: &str = "提交日期";

pub const WORKSPACE_BACK: &str = "返回仓库列表";
pub const WORKSPACE_BACK_ARROW: &str = "←";
pub const WORKSPACE_BACK_TO_WORKFLOWS: &str = "返回工作流";
pub const WORKSPACE_WORKFLOWS: &str = "工作流";
pub const WORKSPACE_RUNS: &str = "运行";
pub const WORKSPACE_RUN_HISTORY: &str = "← 运行记录";
pub const WORKSPACE_RUN_HISTORY_CLOSE: &str = "关闭运行记录 →";
pub const WORKSPACE_BOARD: &str = "发布看板 →";
pub const WORKSPACE_BOARD_CLOSE: &str = "关闭看板";
pub const WORKSPACE_TEMPLATE: &str = "发布模板";
pub const WORKSPACE_TEMPLATE_CLOSE: &str = "关闭模板";

pub const BOARD_VERSION: &str = "版本";
pub const BOARD_VERSION_HINT: &str = "例如 v0.1.0";
pub const BOARD_TARGET: &str = "发布目标";
pub const BOARD_TRIGGER: &str = "触发构建";
pub const BOARD_PUBLISH: &str = "发布";
pub const BOARD_SIMULATED: &str = "模拟";
pub const BOARD_MISSING: &str = "该仓库还没有发布清单（.github/release-console.yml）。";
pub const BOARD_INVALID: &str = "发布清单读不出来";
pub const BOARD_LOADING: &str = "正在读取发布清单…";
pub const BOARD_EMPTY_CELL: &str = "—";
pub const BOARD_TRIGGERED: &str = "已触发构建，等待 runner。";
pub const BOARD_TRIGGER_NO_MANIFEST: &str = "该仓库没有可用的发布清单，无法触发构建。";
pub const BOARD_TRIGGER_UNKNOWN_TARGET: &str = "清单里没有这个发布目标。";
pub const BOARD_PUBLISHED: &str = "已把该版本登记到这条通道。";
pub const BOARD_PUBLISH_NEEDS_ASSETS: &str =
    "该版本还没有这个目标的发布资产，不能登记发布（构建成功不等于已发布）。";
pub const BOARD_PUBLISH_UNKNOWN_TARGET: &str = "清单里没有这个发布目标。";
pub const BOARD_NO_VERSION: &str = "先在上面填一个版本。";
pub const BOARD_STORE_FAILED: &str = "本地记录失败，请重试。";
pub const BOARD_MANIFEST_EDIT: &str = "编辑清单";
pub const BOARD_MANIFEST_CANCEL: &str = "取消编辑";
pub const BOARD_MANIFEST_TITLE: &str = "发布清单";
pub const BOARD_MANIFEST_SUBMIT: &str = "提交为 PR";
pub const BOARD_MANIFEST_NO_TEXT: &str = "还没有读到清单，先刷新再看。";
pub const BOARD_MANIFEST_NO_BASE: &str = "不知道默认分支，无法提交。";
pub const BOARD_MANIFEST_NO_REPOSITORY: &str = "没有选中的仓库，无法提交。";
pub const BOARD_MANIFEST_SUBMITTED: &str = "已提交为 PR";
pub const WORKSPACE_COMING_SOON: &str = "即将推出。";

pub const PROBLEM_RATE_LIMITED: &str = "已触发 GitHub 限流，请稍后重试。";
pub const PROBLEM_NOT_FOUND: &str = "该仓库不存在或无权访问。";

pub const WORKFLOWS_LOADING: &str = "正在加载工作流…";
pub const WORKFLOWS_EMPTY: &str = "该仓库没有工作流。";
pub const WORKFLOWS_PICK: &str = "选择一个工作流查看它的定义。";
pub const WORKFLOW_FILE_LOADING: &str = "正在加载工作流定义…";
pub const WORKFLOW_FILE_EMPTY: &str = "该工作流没有内容。";
pub const WORKFLOW_SAVE: &str = "保存";
pub const WORKFLOW_SAVED: &str = "已保存并提交到默认分支。";
pub const WORKFLOW_SAVE_NO_BRANCH: &str = "无法确定该仓库的默认分支，暂时不能保存。";

pub const WORKFLOW_TEMPLATE_TITLE: &str = "多平台发布模板";
pub const WORKFLOW_TEMPLATE_SAVE: &str = "保存模板";
pub const WORKFLOW_TEMPLATE_SAVED: &str = "已用发布模板新建工作流并推送到默认分支。";
pub const WORKFLOW_TEMPLATE_REPLACED: &str = "已用发布模板覆盖仓库里的同名工作流并推送到默认分支。";
pub const WORKFLOW_TEMPLATE_HINT: &str = "写入 .github/workflows/release-target.yml：Windows / macOS-Intel / macOS-Arm / Linux 一起构建，tag 触发时把产物挂到 Release。文件已存在就覆盖。";

pub const WORKFLOW_NEW: &str = "新建工作流";
pub const WORKFLOW_NEW_CANCEL: &str = "取消新建";
pub const WORKFLOW_NEW_SAVE: &str = "保存";
pub const WORKFLOW_NEW_PREVIEW: &str = "预览";
pub const WORKFLOW_NEW_PUSH: &str = "推送";
pub const WORKFLOW_NEW_BACK: &str = "返回表单";
pub const WORKFLOW_NEW_FILE: &str = "文件名";
pub const WORKFLOW_NEW_NAME: &str = "名称";
pub const WORKFLOW_NEW_RUNNER: &str = "操作系统版本";
pub const WORKFLOW_NEW_TRIGGERS: &str = "触发方式";
pub const WORKFLOW_NEW_TRIGGER_MANUAL: &str = "手动";
pub const WORKFLOW_NEW_TRIGGER_PUSH: &str = "push";
pub const WORKFLOW_NEW_TRIGGER_PULL_REQUEST: &str = "pull_request";
pub const WORKFLOW_NEW_TRIGGER_SCHEDULE: &str = "定时";
pub const WORKFLOW_NEW_CRON: &str = "cron 表达式";
pub const WORKFLOW_NEW_PUSH_BRANCHES: &str = "push 分支";
pub const WORKFLOW_NEW_PULL_REQUEST_BRANCHES: &str = "pull_request 分支";
pub const WORKFLOW_NEW_BRANCHES_HINT: &str =
    "留空表示所有分支，多个用逗号分隔，例如 main, release/*";
pub const WORKFLOW_NEW_CONTAINER: &str = "运行容器";
pub const WORKFLOW_NEW_CONTAINER_HINT: &str = "留空则不用容器，例如 node:20-bullseye";
pub const WORKFLOW_NEW_JOBS: &str = "Job";
pub const WORKFLOW_NEW_ADD_JOB: &str = "添加 Job";
pub const WORKFLOW_NEW_REMOVE_JOB: &str = "删除";
pub const WORKFLOW_NEW_JOB_ID: &str = "标识";
pub const WORKFLOW_NEW_JOB_NAME: &str = "名称";
pub const WORKFLOW_NEW_JOB_COMMAND: &str = "运行命令";
pub const WORKFLOW_NEW_FILE_HINT: &str = "例如 ci，写入 ci.yml";
pub const WORKFLOW_NEW_NAME_HINT: &str = "默认与文件名相同";
pub const WORKFLOW_NEW_JOB_ID_HINT: &str = "例如 build";
pub const WORKFLOW_NEW_JOB_NAME_HINT: &str = "显示名称";
pub const WORKFLOW_NEW_JOB_COMMAND_HINT: &str = "例如 npm test";
pub const WORKFLOW_NEW_CRON_HINT: &str = "例如 0 3 * * *";
pub const WORKFLOW_NEW_PUSHED: &str = "已创建工作流并推送到默认分支。";
pub const WORKFLOW_NEW_REPLACED: &str = "已覆盖仓库里的同名工作流并推送到默认分支。";
pub const WORKFLOW_NEW_USE_TEMPLATE: &str = "使用发布模板";
pub const WORKFLOW_NEW_SAVED_LOCALLY: &str = "已保存到本地";
pub const WORKFLOW_NEW_SAVE_FAILED: &str = "保存到本地失败。";
pub const WORKFLOW_NEW_NO_TRIGGER: &str = "至少选择一种触发方式。";
pub const WORKFLOW_NEW_BAD_SCHEDULE: &str = "定时触发需要一个 cron 表达式。";
pub const WORKFLOW_NEW_BAD_FILE: &str = "文件名不合法：只能包含字母、数字、点、减号和下划线。";
pub const WORKFLOW_NEW_BAD_JOB: &str =
    "Job 标识不合法：需以字母或下划线开头，之后只能用字母、数字、减号和下划线。";
pub const WORKFLOW_NEW_NO_JOBS: &str = "至少需要一个 Job。";
pub const WORKFLOW_NEW_NO_REPOSITORY: &str = "还没有打开仓库。";
pub const WORKFLOW_NEW_NO_BRANCH: &str = "无法确定该仓库的默认分支，暂时不能推送。";
pub const WORKFLOW_RUN: &str = "运行";
pub const WORKFLOW_RUN_NO_SELECTION: &str = "请先选择一个工作流。";
pub const WORKFLOW_RUN_NO_BRANCH: &str = "无法确定该仓库的默认分支，暂时不能触发运行。";
pub const WORKFLOW_RUN_TRIGGERED: &str = "已触发工作流运行。";
pub const RUNS_LOADING: &str = "正在加载运行…";
pub const RUNS_EMPTY: &str = "没有匹配的运行。";
pub const RUNS_LOAD_MORE: &str = "加载更多运行";
pub const RUNS_COLUMN_STATUS: &str = "运行状态";
pub const RUNS_COLUMN_WORKFLOW: &str = "工作流名称";
pub const RUNS_COLUMN_CONCLUSION: &str = "运行结果";
pub const RUNS_COLUMN_BRANCH: &str = "分支";
pub const RUNS_COLUMN_EVENT: &str = "触发方式";
pub const RUNS_COLUMN_CREATED: &str = "执行时间";
pub const RUNS_BRANCH_PLACEHOLDER: &str = "按分支过滤";
pub const VALUE_MISSING: &str = "—";

/// The conclusion GitHub reports, in the console's language. Anything the
/// console does not know is shown as GitHub spelled it.
pub fn conclusion_label(conclusion: Option<&str>) -> String {
    let Some(conclusion) = conclusion.filter(|value| !value.is_empty()) else {
        return VALUE_MISSING.to_owned();
    };

    match conclusion {
        "success" => "成功",
        "failure" => "失败",
        "cancelled" => "已取消",
        "skipped" => "已跳过",
        "timed_out" => "超时",
        "action_required" => "需要操作",
        "neutral" => "中性",
        _ => conclusion,
    }
    .to_owned()
}

pub const RUN_DETAIL_BACK: &str = "返回运行列表";
pub const RUN_DETAIL_OPEN_BROWSER: &str = "在浏览器打开";
pub const JOBS_TITLE: &str = "Jobs";
pub const JOBS_LOADING: &str = "正在加载 job…";
pub const JOBS_EMPTY: &str = "该运行没有 job。";
pub const JOB_VIEW_LOGS: &str = "查看日志";
pub const JOB_STEPS: &str = "步骤";
pub const LOGS_TITLE: &str = "日志";
pub const LOGS_LOADING: &str = "正在加载日志…";
pub const LOGS_EMPTY: &str = "该 job 没有日志。";
pub const LOGS_COPY: &str = "复制日志";
pub const LOGS_COPIED: &str = "已复制";
pub const LOGS_SEARCH_PLACEHOLDER: &str = "在日志中搜索";

pub const ARTIFACTS_TITLE: &str = "构建产物";
pub const ARTIFACTS_LOADING: &str = "正在加载构建产物…";
pub const ARTIFACTS_EMPTY: &str = "该运行没有构建产物。";
pub const ARTIFACTS_EXPIRED: &str = "已过期";
pub const ARTIFACT_DOWNLOAD: &str = "下载";
pub const RUN_LOGS_DOWNLOAD: &str = "下载运行日志包";
pub const DOWNLOAD_CONFIRM_TITLE: &str = "该文件较大，确认下载？";
pub const DOWNLOAD_CONFIRM: &str = "确认下载";
pub const DOWNLOAD_CANCEL: &str = "取消";
pub const DOWNLOADING: &str = "正在下载…";
pub const DOWNLOAD_SAVED: &str = "已保存到";

pub const STATUS_BAR_ACCOUNT: &str = "登录用户";
pub const STATUS_BAR_RATE_LIMIT: &str = "限流余量";
pub const STATUS_BAR_SIGNED_OUT: &str = "未登录";
pub const NOTICES_DISMISS: &str = "清除通知";

pub const SETTINGS_TITLE: &str = "设置";
pub const SETTINGS_GROUP_TITLE: &str = "网络";
pub const SETTINGS_PROXY_LABEL: &str = "代理地址";
pub const SETTINGS_PROXY_PLACEHOLDER: &str = "例如 http://127.0.0.1:7890";
pub const SETTINGS_SAVE: &str = "保存";
pub const SETTINGS_CLOSE: &str = "关闭";

pub const RESUME_TITLE: &str = "登录仍然有效";
pub const RESUME_BODY: &str = "上次登录仍然有效，是否直接进入应用？";
pub const RESUME_YES: &str = "是";
pub const RESUME_NO: &str = "否";

pub const RELEASE_FACTS_TITLE: &str = "发布事实";
pub const RELEASE_FACTS_VERSION: &str = "版本";
pub const RELEASE_FACTS_TARGET: &str = "发布目标";
pub const RELEASE_FACTS_CONFIG: &str = "打包配置";
pub const RELEASE_FACTS_BUILT: &str = "构建完成";
pub const RELEASE_FACTS_ASSETS: &str = "产物可获取";
pub const RELEASE_FACTS_PUBLISHED: &str = "已登记发布";
pub const RELEASE_FACTS_UNKNOWN: &str =
    "这次运行不是控制台触发的，无法确定它对应的发布版本与目标。";
pub const RELEASE_FACTS_NOTHING: &str = "还没有";
pub const RELEASE_FACTS_CHANNEL: &str = "通道";

pub const RELEASE_ASSETS_LABEL: &str = "发布资产";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conclusions_read_the_way_the_console_speaks() {
        assert_eq!(conclusion_label(Some("success")), "成功");
        assert_eq!(conclusion_label(Some("failure")), "失败");
        assert_eq!(conclusion_label(Some("cancelled")), "已取消");
        assert_eq!(conclusion_label(Some("timed_out")), "超时");
        // GitHub's own spelling stands for anything the console does not know.
        assert_eq!(conclusion_label(Some("stale")), "stale");
        assert_eq!(conclusion_label(None), VALUE_MISSING);
        assert_eq!(conclusion_label(Some("")), VALUE_MISSING);
    }
}
