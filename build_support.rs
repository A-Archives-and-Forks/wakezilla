pub fn frontend_build_is_required(
    is_release: bool,
    frontend_index_exists: bool,
    use_prebuilt_frontend: bool,
) -> bool {
    !frontend_index_exists || (is_release && !use_prebuilt_frontend)
}
