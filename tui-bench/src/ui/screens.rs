/// 画面の種類
#[derive(Clone, Debug)]
pub enum Screen {
    /// プロジェクト一覧画面
    ProjectList {
        selected_index: usize,
    },
    /// Quick Add画面（現在のディレクトリを登録）
    QuickAdd {
        name: String,
        path: String,
        aliases: String,
    },
    /// 追加フォーム画面
    AddForm {
        name: String,
        path: String,
        aliases: String,
        tags: String,
        language: String,
        current_field: usize,
    },
    /// 編集フォーム画面
    EditForm {
        index: usize,
        name: String,
        path: String,
        aliases: String,
        tags: String,
        language: String,
        current_field: usize,
    },
    /// 削除確認画面
    DeleteConfirm {
        index: usize,
        project_name: String,
    },
}

impl Screen {
    /// プロジェクト一覧画面を作成
    pub fn project_list() -> Self {
        Screen::ProjectList {
            selected_index: 0,
        }
    }

    /// 指定インデックスを選択した状態でプロジェクト一覧画面を作成
    pub fn project_list_at(index: usize) -> Self {
        Screen::ProjectList {
            selected_index: index,
        }
    }

    /// Quick Add画面を作成
    pub fn quick_add(name: String, path: String) -> Self {
        Screen::QuickAdd {
            name,
            path,
            aliases: String::new(),
        }
    }

    /// 追加フォーム画面を作成
    pub fn add_form() -> Self {
        Screen::AddForm {
            name: String::new(),
            path: String::new(),
            aliases: String::new(),
            tags: String::new(),
            language: String::new(),
            current_field: 0,
        }
    }
}

/// フォームフィールドの定義
#[derive(Clone, Copy, Debug)]
pub enum FormField {
    Name,
    Path,
    Aliases,
    Tags,
    Language,
}

impl FormField {
    /// 全フィールドを取得
    pub fn all() -> &'static [FormField] {
        &[FormField::Name, FormField::Path, FormField::Aliases, FormField::Tags, FormField::Language]
    }

    /// フィールド数
    pub fn count() -> usize {
        Self::all().len()
    }

    /// ラベルを取得
    pub fn label(&self) -> &'static str {
        match self {
            FormField::Name => "名前",
            FormField::Path => "パス",
            FormField::Aliases => "エイリアス (カンマ区切り)",
            FormField::Tags => "タグ (カンマ区切り)",
            FormField::Language => "言語",
        }
    }

    /// プレースホルダーを取得
    pub fn placeholder(&self) -> &'static str {
        match self {
            FormField::Name => "プロジェクト名",
            FormField::Path => "/path/to/project",
            FormField::Aliases => "alias1, alias2",
            FormField::Tags => "tag1, tag2",
            FormField::Language => "TypeScript",
        }
    }
}
