use crate::input_field::InputField;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateProjectField {
	Name,
	Dir,
	Url,
	Entry,
}

#[derive(Debug, Clone)]
pub struct CreateProjectState {
	pub name: InputField,
	pub dir: InputField,
	pub url: InputField,
	pub entry: InputField,
	pub focused_field: CreateProjectField,
	pub invalid_fields: Vec<CreateProjectField>,
	pub error: Option<String>,
}

impl Default for CreateProjectState {
	fn default() -> Self {
		Self {
			name: InputField::new(),
			dir: InputField::new(),
			url: InputField::new(),
			entry: InputField::new(),
			focused_field: CreateProjectField::Name,
			invalid_fields: Vec::new(),
			error: None,
		}
	}
}
