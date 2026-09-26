//! 动作的防抖与"在飞"标记。
//!
//! GPUI 会把每一次按下都当成一次点击，所以双击就是两次点击。落到网络上的动作不能
//! 这样重复：一次推送变两次提交、一次运行变两次 dispatch、一次发布变两条指针。这里
//! 用两种办法挡住：
//!
//! - **在飞**：动作还没回来之前，同一个动作再点不算数（按钮同时置灰，能看见）。
//! - **防抖**：开关类动作靠得太近的第二下不算数，双击不会开一下又关掉。

use std::time::{Duration, Instant};

use super::*;

/// 两次点击之间的最小间隔：比这更近的第二下当成同一次点击的余波。
const DEBOUNCE: Duration = Duration::from_millis(350);

/// 一个受保护的动作。按键而不是按按钮分：同一种动作同时只允许一次。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ActionKey {
    /// 左列工作流条目的运行。
    Run,
    /// 看板格子的发布（登记通道指针）。
    Publish,
    /// 看板的触发构建。
    TriggerBuild,
    /// 新建工作流表单的推送。
    PushWorkflow,
    /// 编辑器右下角的保存：工作流文件。
    SaveWorkflow,
    /// 编辑器右下角的保存：发布模板。
    SaveReleaseFlow,
    /// 采用发布模板直接建一条（表单里那颗）。
    UseTemplate,
    /// 清单编辑器的提交为 PR。
    SubmitManifest,
    /// 仓库列表的刷新。
    RefreshRepositories,
    /// 进入某个仓库（双击不该进两次）。
    ChooseRepository,
    /// 仓库列表的加载更多。
    LoadMoreRepositories,
    /// 运行记录的加载更多。
    LoadMoreRuns,
    /// 运行记录里的刷新。
    RefreshRuns,
    /// 取消一次运行。
    CancelRun,
    /// 删除一次运行。
    DeleteRun,
    /// 新建工作流的表单开关。
    NewWorkflowForm,
    /// 新建发布流 / 取消新建发布流的开关。
    ReleaseFlow,
    /// 看板里清单编辑器的开关。
    ManifestEditor,
}

impl AppView {
    /// 这个动作还在飞：已经点下去了，结果还没回来。
    pub(super) fn action_in_flight(&self, action: ActionKey) -> bool {
        self.in_flight.contains(&action)
    }

    /// 认领一次动作。同一个动作已经在飞就返回 false，这一次点击不算数。
    pub(super) fn begin_action(&mut self, action: ActionKey) -> bool {
        self.in_flight.insert(action)
    }

    /// 动作结束（成了也算，败了也算），放开这一格。
    pub(super) fn end_action(&mut self, action: ActionKey, cx: &mut Context<Self>) {
        if self.in_flight.remove(&action) {
            cx.notify();
        }
    }

    /// 异步体结尾用的那一句：放开"在飞"标记。异步体里拿不到 `&mut self`，所以走
    /// 弱引用回主线程改。
    pub(super) fn release_action(this: &WeakEntity<AppView>, action: ActionKey, cx: &mut AsyncApp) {
        if let Err(error) = this.update(cx, |this, cx| this.end_action(action, cx)) {
            warn!(?error, "the view was gone before the update landed");
        }
    }

    /// 开关类动作的防抖：靠得太近的第二下不算数。返回 true 表示这一下算数。
    pub(super) fn accept_click(&mut self, action: ActionKey) -> bool {
        let now = Instant::now();
        if !is_new_click(self.last_clicked.get(&action).copied(), now) {
            return false;
        }
        self.last_clicked.insert(action, now);
        true
    }
}

/// 上一次点火到现在够不够久。够久才算新的一次点击。
fn is_new_click(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.duration_since(last) >= DEBOUNCE)
}

#[cfg(test)]
mod tests {
    // 只借这两个：`use super::*` 会把 `gpui_kit::test` 也带进来，把自己写的
    // `#[test]` 顶掉。
    use std::time::{Duration, Instant};

    use super::{DEBOUNCE, is_new_click};

    #[test]
    fn the_first_click_is_always_accepted() {
        assert!(is_new_click(None, Instant::now()));
    }

    #[test]
    fn a_second_click_too_soon_is_swallowed() {
        let first = Instant::now();

        assert!(!is_new_click(
            Some(first),
            first + Duration::from_millis(120)
        ));
        assert!(!is_new_click(
            Some(first),
            first + DEBOUNCE - Duration::from_millis(1)
        ));
    }

    #[test]
    fn a_click_after_the_window_is_a_new_one() {
        let first = Instant::now();

        assert!(is_new_click(Some(first), first + DEBOUNCE));
        assert!(is_new_click(
            Some(first),
            first + DEBOUNCE + Duration::from_millis(1)
        ));
        assert!(is_new_click(Some(first), first + Duration::from_secs(2)));
    }
}
