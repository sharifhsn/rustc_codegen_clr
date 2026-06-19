pub mod Microsoft{
pub mod Win32{
pub mod SafeHandles{
pub type CriticalHandleMinusOneIsInvalid =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.CriticalHandleMinusOneIsInvalid">;
use super::super::super::*;
impl From<CriticalHandleMinusOneIsInvalid> for System::Runtime::InteropServices::CriticalHandle {
 fn from(v:CriticalHandleMinusOneIsInvalid)->System::Runtime::InteropServices::CriticalHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::CriticalHandle,CriticalHandleMinusOneIsInvalid>(v)
}} 
impl CriticalHandleMinusOneIsInvalid {
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
}
pub type CriticalHandleZeroOrMinusOneIsInvalid =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.CriticalHandleZeroOrMinusOneIsInvalid">;
use super::super::super::*;
impl From<CriticalHandleZeroOrMinusOneIsInvalid> for System::Runtime::InteropServices::CriticalHandle {
 fn from(v:CriticalHandleZeroOrMinusOneIsInvalid)->System::Runtime::InteropServices::CriticalHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::CriticalHandle,CriticalHandleZeroOrMinusOneIsInvalid>(v)
}} 
impl CriticalHandleZeroOrMinusOneIsInvalid {
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
}
pub type SafeHandleMinusOneIsInvalid =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.SafeHandleMinusOneIsInvalid">;
use super::super::super::*;
impl From<SafeHandleMinusOneIsInvalid> for System::Runtime::InteropServices::SafeHandle {
 fn from(v:SafeHandleMinusOneIsInvalid)->System::Runtime::InteropServices::SafeHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::SafeHandle,SafeHandleMinusOneIsInvalid>(v)
}} 
impl SafeHandleMinusOneIsInvalid {
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
}
pub type SafeHandleZeroOrMinusOneIsInvalid =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.SafeHandleZeroOrMinusOneIsInvalid">;
use super::super::super::*;
impl From<SafeHandleZeroOrMinusOneIsInvalid> for System::Runtime::InteropServices::SafeHandle {
 fn from(v:SafeHandleZeroOrMinusOneIsInvalid)->System::Runtime::InteropServices::SafeHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::SafeHandle,SafeHandleZeroOrMinusOneIsInvalid>(v)
}} 
impl SafeHandleZeroOrMinusOneIsInvalid {
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
}
pub type SafeFileHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.SafeFileHandle">;
use super::super::super::*;
impl From<SafeFileHandle> for Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid {
 fn from(v:SafeFileHandle)->Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid,SafeFileHandle>(v)
}} 
impl SafeFileHandle {
    pub fn get_is_async(self) -> bool { self.instance0::<"get_IsAsync", bool>() }
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
    pub fn new(a1: isize, a2: bool) -> Self { Self::ctor2(a1, a2) }
}
pub type SafeWaitHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Microsoft.Win32.SafeHandles.SafeWaitHandle">;
use super::super::super::*;
impl From<SafeWaitHandle> for Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid {
 fn from(v:SafeWaitHandle)->Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid,SafeWaitHandle>(v)
}} 
impl SafeWaitHandle {
    pub fn new() -> Self { Self::ctor0() }
}
}
}
}
pub mod System{
pub mod Numerics{
pub type BitOperations =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Numerics.BitOperations">;
use super::super::*;
impl From<BitOperations> for System::Object {
 fn from(v:BitOperations)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BitOperations>(v)
}} 
impl BitOperations {
    pub fn is_pow2(a1: i32) -> bool { Self::static1::<"IsPow2", i32, bool>(a1) }
    pub fn round_up_to_power_of2(a1: u32) -> u32 { Self::static1::<"RoundUpToPowerOf2", u32, u32>(a1) }
    pub fn leading_zero_count(a1: u32) -> i32 { Self::static1::<"LeadingZeroCount", u32, i32>(a1) }
    pub fn log2(a1: u32) -> i32 { Self::static1::<"Log2", u32, i32>(a1) }
    pub fn pop_count(a1: u32) -> i32 { Self::static1::<"PopCount", u32, i32>(a1) }
    pub fn trailing_zero_count(a1: i32) -> i32 { Self::static1::<"TrailingZeroCount", i32, i32>(a1) }
    pub fn rotate_left(a1: u32, a2: i32) -> u32 { Self::static2::<"RotateLeft", u32, i32, u32>(a1, a2) }
    pub fn rotate_right(a1: u32, a2: i32) -> u32 { Self::static2::<"RotateRight", u32, i32, u32>(a1, a2) }
    pub fn crc32_c(a1: u32, a2: u8) -> u32 { Self::static2::<"Crc32C", u32, u8, u32>(a1, a2) }
}
pub type Vector =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Numerics.Vector">;
use super::super::*;
impl From<Vector> for System::Object {
 fn from(v:Vector)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Vector>(v)
}} 
impl Vector {
    pub fn get_is_hardware_accelerated() -> bool { Self::static0::<"get_IsHardwareAccelerated", bool>() }
}
}
pub mod Net{
pub type WebUtility =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Net.WebUtility">;
use super::super::*;
impl From<WebUtility> for System::Object {
 fn from(v:WebUtility)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,WebUtility>(v)
}} 
impl WebUtility {
    pub fn html_encode(a1: System::String) -> System::String { Self::static1::<"HtmlEncode", System::String, System::String>(a1) }
    pub fn html_decode(a1: System::String) -> System::String { Self::static1::<"HtmlDecode", System::String, System::String>(a1) }
    pub fn url_encode(a1: System::String) -> System::String { Self::static1::<"UrlEncode", System::String, System::String>(a1) }
    pub fn url_decode(a1: System::String) -> System::String { Self::static1::<"UrlDecode", System::String, System::String>(a1) }
}
}
pub mod Globalization{
pub type Calendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.Calendar">;
use super::super::*;
impl From<Calendar> for System::Object {
 fn from(v:Calendar)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Calendar>(v)
}} 
impl Calendar {
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn read_only(a1: System::Globalization::Calendar) -> System::Globalization::Calendar { Self::static1::<"ReadOnly", System::Globalization::Calendar, System::Globalization::Calendar>(a1) }
    pub fn get_days_in_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInMonth", i32, i32, i32>(a1, a2) }
    pub fn get_days_in_year(self, a1: i32) -> i32 { self.instance1::<"GetDaysInYear", i32, i32>(a1) }
    pub fn get_months_in_year(self, a1: i32) -> i32 { self.instance1::<"GetMonthsInYear", i32, i32>(a1) }
    pub fn is_leap_month(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapMonth", i32, i32, bool>(a1, a2) }
    pub fn get_leap_month(self, a1: i32) -> i32 { self.instance1::<"GetLeapMonth", i32, i32>(a1) }
    pub fn is_leap_year(self, a1: i32) -> bool { self.instance1::<"IsLeapYear", i32, bool>(a1) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
}
pub type CharUnicodeInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.CharUnicodeInfo">;
use super::super::*;
impl From<CharUnicodeInfo> for System::Object {
 fn from(v:CharUnicodeInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CharUnicodeInfo>(v)
}} 
impl CharUnicodeInfo {
    pub fn get_decimal_digit_value(a1: System::String, a2: i32) -> i32 { Self::static2::<"GetDecimalDigitValue", System::String, i32, i32>(a1, a2) }
    pub fn get_digit_value(a1: System::String, a2: i32) -> i32 { Self::static2::<"GetDigitValue", System::String, i32, i32>(a1, a2) }
    pub fn get_numeric_value(a1: System::String, a2: i32) -> f64 { Self::static2::<"GetNumericValue", System::String, i32, f64>(a1, a2) }
}
pub type ChineseLunisolarCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.ChineseLunisolarCalendar">;
use super::super::*;
impl From<ChineseLunisolarCalendar> for System::Globalization::EastAsianLunisolarCalendar {
 fn from(v:ChineseLunisolarCalendar)->System::Globalization::EastAsianLunisolarCalendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::EastAsianLunisolarCalendar,ChineseLunisolarCalendar>(v)
}} 
impl ChineseLunisolarCalendar {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CompareInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.CompareInfo">;
use super::super::*;
impl From<CompareInfo> for System::Object {
 fn from(v:CompareInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CompareInfo>(v)
}} 
impl CompareInfo {
    pub fn get_compare_info(a1: i32, a2: System::Reflection::Assembly) -> System::Globalization::CompareInfo { Self::static2::<"GetCompareInfo", i32, System::Reflection::Assembly, System::Globalization::CompareInfo>(a1, a2) }
    pub fn is_sortable(a1: System::String) -> bool { Self::static1::<"IsSortable", System::String, bool>(a1) }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn compare(self, a1: System::String, a2: System::String) -> i32 { self.instance2::<"Compare", System::String, System::String, i32>(a1, a2) }
    pub fn is_prefix(self, a1: System::String, a2: System::String) -> bool { self.instance2::<"IsPrefix", System::String, System::String, bool>(a1, a2) }
    pub fn is_suffix(self, a1: System::String, a2: System::String) -> bool { self.instance2::<"IsSuffix", System::String, System::String, bool>(a1, a2) }
    pub fn index_of(self, a1: System::String, a2: System::String) -> i32 { self.instance2::<"IndexOf", System::String, System::String, i32>(a1, a2) }
    pub fn last_index_of(self, a1: System::String, a2: System::String) -> i32 { self.instance2::<"LastIndexOf", System::String, System::String, i32>(a1, a2) }
    pub fn get_sort_key(self, a1: System::String) -> System::Globalization::SortKey { self.instance1::<"GetSortKey", System::String, System::Globalization::SortKey>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_version(self) -> System::Globalization::SortVersion { self.instance0::<"get_Version", System::Globalization::SortVersion>() }
    pub fn get_lcid(self) -> i32 { self.instance0::<"get_LCID", i32>() }
}
pub type CultureInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.CultureInfo">;
use super::super::*;
impl From<CultureInfo> for System::Object {
 fn from(v:CultureInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CultureInfo>(v)
}} 
impl CultureInfo {
    pub fn create_specific_culture(a1: System::String) -> System::Globalization::CultureInfo { Self::static1::<"CreateSpecificCulture", System::String, System::Globalization::CultureInfo>(a1) }
    pub fn get_current_culture() -> System::Globalization::CultureInfo { Self::static0::<"get_CurrentCulture", System::Globalization::CultureInfo>() }
    pub fn set_current_culture(a1: System::Globalization::CultureInfo) { Self::static1::<"set_CurrentCulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_current_uiculture() -> System::Globalization::CultureInfo { Self::static0::<"get_CurrentUICulture", System::Globalization::CultureInfo>() }
    pub fn set_current_uiculture(a1: System::Globalization::CultureInfo) { Self::static1::<"set_CurrentUICulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_installed_uiculture() -> System::Globalization::CultureInfo { Self::static0::<"get_InstalledUICulture", System::Globalization::CultureInfo>() }
    pub fn get_default_thread_current_culture() -> System::Globalization::CultureInfo { Self::static0::<"get_DefaultThreadCurrentCulture", System::Globalization::CultureInfo>() }
    pub fn set_default_thread_current_culture(a1: System::Globalization::CultureInfo) { Self::static1::<"set_DefaultThreadCurrentCulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_default_thread_current_uiculture() -> System::Globalization::CultureInfo { Self::static0::<"get_DefaultThreadCurrentUICulture", System::Globalization::CultureInfo>() }
    pub fn set_default_thread_current_uiculture(a1: System::Globalization::CultureInfo) { Self::static1::<"set_DefaultThreadCurrentUICulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_invariant_culture() -> System::Globalization::CultureInfo { Self::static0::<"get_InvariantCulture", System::Globalization::CultureInfo>() }
    pub fn get_parent(self) -> System::Globalization::CultureInfo { self.virt0::<"get_Parent", System::Globalization::CultureInfo>() }
    pub fn get_lcid(self) -> i32 { self.virt0::<"get_LCID", i32>() }
    pub fn get_keyboard_layout_id(self) -> i32 { self.virt0::<"get_KeyboardLayoutId", i32>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_ietf_language_tag(self) -> System::String { self.instance0::<"get_IetfLanguageTag", System::String>() }
    pub fn get_display_name(self) -> System::String { self.virt0::<"get_DisplayName", System::String>() }
    pub fn get_native_name(self) -> System::String { self.virt0::<"get_NativeName", System::String>() }
    pub fn get_english_name(self) -> System::String { self.virt0::<"get_EnglishName", System::String>() }
    pub fn get_two_letter_isolanguage_name(self) -> System::String { self.virt0::<"get_TwoLetterISOLanguageName", System::String>() }
    pub fn get_three_letter_isolanguage_name(self) -> System::String { self.virt0::<"get_ThreeLetterISOLanguageName", System::String>() }
    pub fn get_three_letter_windows_language_name(self) -> System::String { self.virt0::<"get_ThreeLetterWindowsLanguageName", System::String>() }
    pub fn get_compare_info(self) -> System::Globalization::CompareInfo { self.virt0::<"get_CompareInfo", System::Globalization::CompareInfo>() }
    pub fn get_text_info(self) -> System::Globalization::TextInfo { self.virt0::<"get_TextInfo", System::Globalization::TextInfo>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_format(self, a1: System::Type) -> System::Object { self.instance1::<"GetFormat", System::Type, System::Object>(a1) }
    pub fn get_is_neutral_culture(self) -> bool { self.virt0::<"get_IsNeutralCulture", bool>() }
    pub fn get_number_format(self) -> System::Globalization::NumberFormatInfo { self.virt0::<"get_NumberFormat", System::Globalization::NumberFormatInfo>() }
    pub fn set_number_format(self, a1: System::Globalization::NumberFormatInfo) { self.instance1::<"set_NumberFormat", System::Globalization::NumberFormatInfo, ()>(a1) }
    pub fn get_date_time_format(self) -> System::Globalization::DateTimeFormatInfo { self.virt0::<"get_DateTimeFormat", System::Globalization::DateTimeFormatInfo>() }
    pub fn set_date_time_format(self, a1: System::Globalization::DateTimeFormatInfo) { self.instance1::<"set_DateTimeFormat", System::Globalization::DateTimeFormatInfo, ()>(a1) }
    pub fn clear_cached_data(self) { self.instance0::<"ClearCachedData", ()>() }
    pub fn get_calendar(self) -> System::Globalization::Calendar { self.virt0::<"get_Calendar", System::Globalization::Calendar>() }
    pub fn get_use_user_override(self) -> bool { self.instance0::<"get_UseUserOverride", bool>() }
    pub fn get_console_fallback_uiculture(self) -> System::Globalization::CultureInfo { self.instance0::<"GetConsoleFallbackUICulture", System::Globalization::CultureInfo>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn read_only(a1: System::Globalization::CultureInfo) -> System::Globalization::CultureInfo { Self::static1::<"ReadOnly", System::Globalization::CultureInfo, System::Globalization::CultureInfo>(a1) }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn get_culture_info(a1: i32) -> System::Globalization::CultureInfo { Self::static1::<"GetCultureInfo", i32, System::Globalization::CultureInfo>(a1) }
    pub fn get_culture_info_by_ietf_language_tag(a1: System::String) -> System::Globalization::CultureInfo { Self::static1::<"GetCultureInfoByIetfLanguageTag", System::String, System::Globalization::CultureInfo>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type CultureNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.CultureNotFoundException">;
use super::super::*;
impl From<CultureNotFoundException> for System::ArgumentException {
 fn from(v:CultureNotFoundException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,CultureNotFoundException>(v)
}} 
impl CultureNotFoundException {
    pub fn get_invalid_culture_name(self) -> System::String { self.virt0::<"get_InvalidCultureName", System::String>() }
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DateTimeFormatInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.DateTimeFormatInfo">;
use super::super::*;
impl From<DateTimeFormatInfo> for System::Object {
 fn from(v:DateTimeFormatInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DateTimeFormatInfo>(v)
}} 
impl DateTimeFormatInfo {
    pub fn get_invariant_info() -> System::Globalization::DateTimeFormatInfo { Self::static0::<"get_InvariantInfo", System::Globalization::DateTimeFormatInfo>() }
    pub fn get_current_info() -> System::Globalization::DateTimeFormatInfo { Self::static0::<"get_CurrentInfo", System::Globalization::DateTimeFormatInfo>() }
    pub fn get_instance(a1: System::IFormatProvider) -> System::Globalization::DateTimeFormatInfo { Self::static1::<"GetInstance", System::IFormatProvider, System::Globalization::DateTimeFormatInfo>(a1) }
    pub fn get_format(self, a1: System::Type) -> System::Object { self.instance1::<"GetFormat", System::Type, System::Object>(a1) }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_amdesignator(self) -> System::String { self.instance0::<"get_AMDesignator", System::String>() }
    pub fn set_amdesignator(self, a1: System::String) { self.instance1::<"set_AMDesignator", System::String, ()>(a1) }
    pub fn get_calendar(self) -> System::Globalization::Calendar { self.instance0::<"get_Calendar", System::Globalization::Calendar>() }
    pub fn set_calendar(self, a1: System::Globalization::Calendar) { self.instance1::<"set_Calendar", System::Globalization::Calendar, ()>(a1) }
    pub fn get_era(self, a1: System::String) -> i32 { self.instance1::<"GetEra", System::String, i32>(a1) }
    pub fn get_era_name(self, a1: i32) -> System::String { self.instance1::<"GetEraName", i32, System::String>(a1) }
    pub fn get_abbreviated_era_name(self, a1: i32) -> System::String { self.instance1::<"GetAbbreviatedEraName", i32, System::String>(a1) }
    pub fn get_date_separator(self) -> System::String { self.instance0::<"get_DateSeparator", System::String>() }
    pub fn set_date_separator(self, a1: System::String) { self.instance1::<"set_DateSeparator", System::String, ()>(a1) }
    pub fn get_full_date_time_pattern(self) -> System::String { self.instance0::<"get_FullDateTimePattern", System::String>() }
    pub fn set_full_date_time_pattern(self, a1: System::String) { self.instance1::<"set_FullDateTimePattern", System::String, ()>(a1) }
    pub fn get_long_date_pattern(self) -> System::String { self.instance0::<"get_LongDatePattern", System::String>() }
    pub fn set_long_date_pattern(self, a1: System::String) { self.instance1::<"set_LongDatePattern", System::String, ()>(a1) }
    pub fn get_long_time_pattern(self) -> System::String { self.instance0::<"get_LongTimePattern", System::String>() }
    pub fn set_long_time_pattern(self, a1: System::String) { self.instance1::<"set_LongTimePattern", System::String, ()>(a1) }
    pub fn get_month_day_pattern(self) -> System::String { self.instance0::<"get_MonthDayPattern", System::String>() }
    pub fn set_month_day_pattern(self, a1: System::String) { self.instance1::<"set_MonthDayPattern", System::String, ()>(a1) }
    pub fn get_pmdesignator(self) -> System::String { self.instance0::<"get_PMDesignator", System::String>() }
    pub fn set_pmdesignator(self, a1: System::String) { self.instance1::<"set_PMDesignator", System::String, ()>(a1) }
    pub fn get_rfc1123_pattern(self) -> System::String { self.instance0::<"get_RFC1123Pattern", System::String>() }
    pub fn get_short_date_pattern(self) -> System::String { self.instance0::<"get_ShortDatePattern", System::String>() }
    pub fn set_short_date_pattern(self, a1: System::String) { self.instance1::<"set_ShortDatePattern", System::String, ()>(a1) }
    pub fn get_short_time_pattern(self) -> System::String { self.instance0::<"get_ShortTimePattern", System::String>() }
    pub fn set_short_time_pattern(self, a1: System::String) { self.instance1::<"set_ShortTimePattern", System::String, ()>(a1) }
    pub fn get_sortable_date_time_pattern(self) -> System::String { self.instance0::<"get_SortableDateTimePattern", System::String>() }
    pub fn get_time_separator(self) -> System::String { self.instance0::<"get_TimeSeparator", System::String>() }
    pub fn set_time_separator(self, a1: System::String) { self.instance1::<"set_TimeSeparator", System::String, ()>(a1) }
    pub fn get_universal_sortable_date_time_pattern(self) -> System::String { self.instance0::<"get_UniversalSortableDateTimePattern", System::String>() }
    pub fn get_year_month_pattern(self) -> System::String { self.instance0::<"get_YearMonthPattern", System::String>() }
    pub fn set_year_month_pattern(self, a1: System::String) { self.instance1::<"set_YearMonthPattern", System::String, ()>(a1) }
    pub fn get_abbreviated_month_name(self, a1: i32) -> System::String { self.instance1::<"GetAbbreviatedMonthName", i32, System::String>(a1) }
    pub fn get_month_name(self, a1: i32) -> System::String { self.instance1::<"GetMonthName", i32, System::String>(a1) }
    pub fn read_only(a1: System::Globalization::DateTimeFormatInfo) -> System::Globalization::DateTimeFormatInfo { Self::static1::<"ReadOnly", System::Globalization::DateTimeFormatInfo, System::Globalization::DateTimeFormatInfo>(a1) }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn get_native_calendar_name(self) -> System::String { self.instance0::<"get_NativeCalendarName", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DaylightTime =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.DaylightTime">;
use super::super::*;
impl From<DaylightTime> for System::Object {
 fn from(v:DaylightTime)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DaylightTime>(v)
}} 
pub type EastAsianLunisolarCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.EastAsianLunisolarCalendar">;
use super::super::*;
impl From<EastAsianLunisolarCalendar> for System::Globalization::Calendar {
 fn from(v:EastAsianLunisolarCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,EastAsianLunisolarCalendar>(v)
}} 
impl EastAsianLunisolarCalendar {
    pub fn get_celestial_stem(self, a1: i32) -> i32 { self.instance1::<"GetCelestialStem", i32, i32>(a1) }
    pub fn get_terrestrial_branch(self, a1: i32) -> i32 { self.instance1::<"GetTerrestrialBranch", i32, i32>(a1) }
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
}
pub type GlobalizationExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.GlobalizationExtensions">;
use super::super::*;
impl From<GlobalizationExtensions> for System::Object {
 fn from(v:GlobalizationExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,GlobalizationExtensions>(v)
}} 
pub type GregorianCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.GregorianCalendar">;
use super::super::*;
impl From<GregorianCalendar> for System::Globalization::Calendar {
 fn from(v:GregorianCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,GregorianCalendar>(v)
}} 
impl GregorianCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type HebrewCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.HebrewCalendar">;
use super::super::*;
impl From<HebrewCalendar> for System::Globalization::Calendar {
 fn from(v:HebrewCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,HebrewCalendar>(v)
}} 
impl HebrewCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type HijriCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.HijriCalendar">;
use super::super::*;
impl From<HijriCalendar> for System::Globalization::Calendar {
 fn from(v:HijriCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,HijriCalendar>(v)
}} 
impl HijriCalendar {
    pub fn get_hijri_adjustment(self) -> i32 { self.instance0::<"get_HijriAdjustment", i32>() }
    pub fn set_hijri_adjustment(self, a1: i32) { self.instance1::<"set_HijriAdjustment", i32, ()>(a1) }
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IdnMapping =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.IdnMapping">;
use super::super::*;
impl From<IdnMapping> for System::Object {
 fn from(v:IdnMapping)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,IdnMapping>(v)
}} 
impl IdnMapping {
    pub fn get_allow_unassigned(self) -> bool { self.instance0::<"get_AllowUnassigned", bool>() }
    pub fn set_allow_unassigned(self, a1: bool) { self.instance1::<"set_AllowUnassigned", bool, ()>(a1) }
    pub fn get_use_std3_ascii_rules(self) -> bool { self.instance0::<"get_UseStd3AsciiRules", bool>() }
    pub fn set_use_std3_ascii_rules(self, a1: bool) { self.instance1::<"set_UseStd3AsciiRules", bool, ()>(a1) }
    pub fn get_ascii(self, a1: System::String) -> System::String { self.instance1::<"GetAscii", System::String, System::String>(a1) }
    pub fn get_unicode(self, a1: System::String) -> System::String { self.instance1::<"GetUnicode", System::String, System::String>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ISOWeek =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.ISOWeek">;
use super::super::*;
impl From<ISOWeek> for System::Object {
 fn from(v:ISOWeek)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ISOWeek>(v)
}} 
impl ISOWeek {
    pub fn get_weeks_in_year(a1: i32) -> i32 { Self::static1::<"GetWeeksInYear", i32, i32>(a1) }
}
pub type JapaneseCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.JapaneseCalendar">;
use super::super::*;
impl From<JapaneseCalendar> for System::Globalization::Calendar {
 fn from(v:JapaneseCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,JapaneseCalendar>(v)
}} 
impl JapaneseCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type JapaneseLunisolarCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.JapaneseLunisolarCalendar">;
use super::super::*;
impl From<JapaneseLunisolarCalendar> for System::Globalization::EastAsianLunisolarCalendar {
 fn from(v:JapaneseLunisolarCalendar)->System::Globalization::EastAsianLunisolarCalendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::EastAsianLunisolarCalendar,JapaneseLunisolarCalendar>(v)
}} 
impl JapaneseLunisolarCalendar {
    pub fn new() -> Self { Self::ctor0() }
}
pub type JulianCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.JulianCalendar">;
use super::super::*;
impl From<JulianCalendar> for System::Globalization::Calendar {
 fn from(v:JulianCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,JulianCalendar>(v)
}} 
impl JulianCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type KoreanCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.KoreanCalendar">;
use super::super::*;
impl From<KoreanCalendar> for System::Globalization::Calendar {
 fn from(v:KoreanCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,KoreanCalendar>(v)
}} 
impl KoreanCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type KoreanLunisolarCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.KoreanLunisolarCalendar">;
use super::super::*;
impl From<KoreanLunisolarCalendar> for System::Globalization::EastAsianLunisolarCalendar {
 fn from(v:KoreanLunisolarCalendar)->System::Globalization::EastAsianLunisolarCalendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::EastAsianLunisolarCalendar,KoreanLunisolarCalendar>(v)
}} 
impl KoreanLunisolarCalendar {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NumberFormatInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.NumberFormatInfo">;
use super::super::*;
impl From<NumberFormatInfo> for System::Object {
 fn from(v:NumberFormatInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NumberFormatInfo>(v)
}} 
impl NumberFormatInfo {
    pub fn get_invariant_info() -> System::Globalization::NumberFormatInfo { Self::static0::<"get_InvariantInfo", System::Globalization::NumberFormatInfo>() }
    pub fn get_instance(a1: System::IFormatProvider) -> System::Globalization::NumberFormatInfo { Self::static1::<"GetInstance", System::IFormatProvider, System::Globalization::NumberFormatInfo>(a1) }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_currency_decimal_digits(self) -> i32 { self.instance0::<"get_CurrencyDecimalDigits", i32>() }
    pub fn set_currency_decimal_digits(self, a1: i32) { self.instance1::<"set_CurrencyDecimalDigits", i32, ()>(a1) }
    pub fn get_currency_decimal_separator(self) -> System::String { self.instance0::<"get_CurrencyDecimalSeparator", System::String>() }
    pub fn set_currency_decimal_separator(self, a1: System::String) { self.instance1::<"set_CurrencyDecimalSeparator", System::String, ()>(a1) }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn get_currency_group_separator(self) -> System::String { self.instance0::<"get_CurrencyGroupSeparator", System::String>() }
    pub fn set_currency_group_separator(self, a1: System::String) { self.instance1::<"set_CurrencyGroupSeparator", System::String, ()>(a1) }
    pub fn get_currency_symbol(self) -> System::String { self.instance0::<"get_CurrencySymbol", System::String>() }
    pub fn set_currency_symbol(self, a1: System::String) { self.instance1::<"set_CurrencySymbol", System::String, ()>(a1) }
    pub fn get_current_info() -> System::Globalization::NumberFormatInfo { Self::static0::<"get_CurrentInfo", System::Globalization::NumberFormatInfo>() }
    pub fn get_na_nsymbol(self) -> System::String { self.instance0::<"get_NaNSymbol", System::String>() }
    pub fn set_na_nsymbol(self, a1: System::String) { self.instance1::<"set_NaNSymbol", System::String, ()>(a1) }
    pub fn get_currency_negative_pattern(self) -> i32 { self.instance0::<"get_CurrencyNegativePattern", i32>() }
    pub fn set_currency_negative_pattern(self, a1: i32) { self.instance1::<"set_CurrencyNegativePattern", i32, ()>(a1) }
    pub fn get_number_negative_pattern(self) -> i32 { self.instance0::<"get_NumberNegativePattern", i32>() }
    pub fn set_number_negative_pattern(self, a1: i32) { self.instance1::<"set_NumberNegativePattern", i32, ()>(a1) }
    pub fn get_percent_positive_pattern(self) -> i32 { self.instance0::<"get_PercentPositivePattern", i32>() }
    pub fn set_percent_positive_pattern(self, a1: i32) { self.instance1::<"set_PercentPositivePattern", i32, ()>(a1) }
    pub fn get_percent_negative_pattern(self) -> i32 { self.instance0::<"get_PercentNegativePattern", i32>() }
    pub fn set_percent_negative_pattern(self, a1: i32) { self.instance1::<"set_PercentNegativePattern", i32, ()>(a1) }
    pub fn get_negative_infinity_symbol(self) -> System::String { self.instance0::<"get_NegativeInfinitySymbol", System::String>() }
    pub fn set_negative_infinity_symbol(self, a1: System::String) { self.instance1::<"set_NegativeInfinitySymbol", System::String, ()>(a1) }
    pub fn get_negative_sign(self) -> System::String { self.instance0::<"get_NegativeSign", System::String>() }
    pub fn set_negative_sign(self, a1: System::String) { self.instance1::<"set_NegativeSign", System::String, ()>(a1) }
    pub fn get_number_decimal_digits(self) -> i32 { self.instance0::<"get_NumberDecimalDigits", i32>() }
    pub fn set_number_decimal_digits(self, a1: i32) { self.instance1::<"set_NumberDecimalDigits", i32, ()>(a1) }
    pub fn get_number_decimal_separator(self) -> System::String { self.instance0::<"get_NumberDecimalSeparator", System::String>() }
    pub fn set_number_decimal_separator(self, a1: System::String) { self.instance1::<"set_NumberDecimalSeparator", System::String, ()>(a1) }
    pub fn get_number_group_separator(self) -> System::String { self.instance0::<"get_NumberGroupSeparator", System::String>() }
    pub fn set_number_group_separator(self, a1: System::String) { self.instance1::<"set_NumberGroupSeparator", System::String, ()>(a1) }
    pub fn get_currency_positive_pattern(self) -> i32 { self.instance0::<"get_CurrencyPositivePattern", i32>() }
    pub fn set_currency_positive_pattern(self, a1: i32) { self.instance1::<"set_CurrencyPositivePattern", i32, ()>(a1) }
    pub fn get_positive_infinity_symbol(self) -> System::String { self.instance0::<"get_PositiveInfinitySymbol", System::String>() }
    pub fn set_positive_infinity_symbol(self, a1: System::String) { self.instance1::<"set_PositiveInfinitySymbol", System::String, ()>(a1) }
    pub fn get_positive_sign(self) -> System::String { self.instance0::<"get_PositiveSign", System::String>() }
    pub fn set_positive_sign(self, a1: System::String) { self.instance1::<"set_PositiveSign", System::String, ()>(a1) }
    pub fn get_percent_decimal_digits(self) -> i32 { self.instance0::<"get_PercentDecimalDigits", i32>() }
    pub fn set_percent_decimal_digits(self, a1: i32) { self.instance1::<"set_PercentDecimalDigits", i32, ()>(a1) }
    pub fn get_percent_decimal_separator(self) -> System::String { self.instance0::<"get_PercentDecimalSeparator", System::String>() }
    pub fn set_percent_decimal_separator(self, a1: System::String) { self.instance1::<"set_PercentDecimalSeparator", System::String, ()>(a1) }
    pub fn get_percent_group_separator(self) -> System::String { self.instance0::<"get_PercentGroupSeparator", System::String>() }
    pub fn set_percent_group_separator(self, a1: System::String) { self.instance1::<"set_PercentGroupSeparator", System::String, ()>(a1) }
    pub fn get_percent_symbol(self) -> System::String { self.instance0::<"get_PercentSymbol", System::String>() }
    pub fn set_percent_symbol(self, a1: System::String) { self.instance1::<"set_PercentSymbol", System::String, ()>(a1) }
    pub fn get_per_mille_symbol(self) -> System::String { self.instance0::<"get_PerMilleSymbol", System::String>() }
    pub fn set_per_mille_symbol(self, a1: System::String) { self.instance1::<"set_PerMilleSymbol", System::String, ()>(a1) }
    pub fn get_format(self, a1: System::Type) -> System::Object { self.instance1::<"GetFormat", System::Type, System::Object>(a1) }
    pub fn read_only(a1: System::Globalization::NumberFormatInfo) -> System::Globalization::NumberFormatInfo { Self::static1::<"ReadOnly", System::Globalization::NumberFormatInfo, System::Globalization::NumberFormatInfo>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type PersianCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.PersianCalendar">;
use super::super::*;
impl From<PersianCalendar> for System::Globalization::Calendar {
 fn from(v:PersianCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,PersianCalendar>(v)
}} 
impl PersianCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type RegionInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.RegionInfo">;
use super::super::*;
impl From<RegionInfo> for System::Object {
 fn from(v:RegionInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RegionInfo>(v)
}} 
impl RegionInfo {
    pub fn get_current_region() -> System::Globalization::RegionInfo { Self::static0::<"get_CurrentRegion", System::Globalization::RegionInfo>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_english_name(self) -> System::String { self.virt0::<"get_EnglishName", System::String>() }
    pub fn get_display_name(self) -> System::String { self.virt0::<"get_DisplayName", System::String>() }
    pub fn get_native_name(self) -> System::String { self.virt0::<"get_NativeName", System::String>() }
    pub fn get_two_letter_isoregion_name(self) -> System::String { self.virt0::<"get_TwoLetterISORegionName", System::String>() }
    pub fn get_three_letter_isoregion_name(self) -> System::String { self.virt0::<"get_ThreeLetterISORegionName", System::String>() }
    pub fn get_three_letter_windows_region_name(self) -> System::String { self.virt0::<"get_ThreeLetterWindowsRegionName", System::String>() }
    pub fn get_is_metric(self) -> bool { self.virt0::<"get_IsMetric", bool>() }
    pub fn get_geo_id(self) -> i32 { self.virt0::<"get_GeoId", i32>() }
    pub fn get_currency_english_name(self) -> System::String { self.virt0::<"get_CurrencyEnglishName", System::String>() }
    pub fn get_currency_native_name(self) -> System::String { self.virt0::<"get_CurrencyNativeName", System::String>() }
    pub fn get_currency_symbol(self) -> System::String { self.virt0::<"get_CurrencySymbol", System::String>() }
    pub fn get_isocurrency_symbol(self) -> System::String { self.virt0::<"get_ISOCurrencySymbol", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SortKey =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.SortKey">;
use super::super::*;
impl From<SortKey> for System::Object {
 fn from(v:SortKey)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SortKey>(v)
}} 
impl SortKey {
    pub fn get_original_string(self) -> System::String { self.instance0::<"get_OriginalString", System::String>() }
    pub fn compare(a1: System::Globalization::SortKey, a2: System::Globalization::SortKey) -> i32 { Self::static2::<"Compare", System::Globalization::SortKey, System::Globalization::SortKey, i32>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type SortVersion =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.SortVersion">;
use super::super::*;
impl From<SortVersion> for System::Object {
 fn from(v:SortVersion)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SortVersion>(v)
}} 
impl SortVersion {
    pub fn get_full_version(self) -> i32 { self.instance0::<"get_FullVersion", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Globalization::SortVersion, a2: System::Globalization::SortVersion) -> bool { Self::static2::<"op_Equality", System::Globalization::SortVersion, System::Globalization::SortVersion, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Globalization::SortVersion, a2: System::Globalization::SortVersion) -> bool { Self::static2::<"op_Inequality", System::Globalization::SortVersion, System::Globalization::SortVersion, bool>(a1, a2) }
}
pub type StringInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.StringInfo">;
use super::super::*;
impl From<StringInfo> for System::Object {
 fn from(v:StringInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringInfo>(v)
}} 
impl StringInfo {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn get_string(self) -> System::String { self.instance0::<"get_String", System::String>() }
    pub fn set_string(self, a1: System::String) { self.instance1::<"set_String", System::String, ()>(a1) }
    pub fn get_length_in_text_elements(self) -> i32 { self.instance0::<"get_LengthInTextElements", i32>() }
    pub fn substring_by_text_elements(self, a1: i32) -> System::String { self.instance1::<"SubstringByTextElements", i32, System::String>(a1) }
    pub fn get_next_text_element(a1: System::String) -> System::String { Self::static1::<"GetNextTextElement", System::String, System::String>(a1) }
    pub fn get_next_text_element_length(a1: System::String) -> i32 { Self::static1::<"GetNextTextElementLength", System::String, i32>(a1) }
    pub fn get_text_element_enumerator(a1: System::String) -> System::Globalization::TextElementEnumerator { Self::static1::<"GetTextElementEnumerator", System::String, System::Globalization::TextElementEnumerator>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaiwanCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.TaiwanCalendar">;
use super::super::*;
impl From<TaiwanCalendar> for System::Globalization::Calendar {
 fn from(v:TaiwanCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,TaiwanCalendar>(v)
}} 
impl TaiwanCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaiwanLunisolarCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.TaiwanLunisolarCalendar">;
use super::super::*;
impl From<TaiwanLunisolarCalendar> for System::Globalization::EastAsianLunisolarCalendar {
 fn from(v:TaiwanLunisolarCalendar)->System::Globalization::EastAsianLunisolarCalendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::EastAsianLunisolarCalendar,TaiwanLunisolarCalendar>(v)
}} 
impl TaiwanLunisolarCalendar {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TextElementEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.TextElementEnumerator">;
use super::super::*;
impl From<TextElementEnumerator> for System::Object {
 fn from(v:TextElementEnumerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TextElementEnumerator>(v)
}} 
impl TextElementEnumerator {
    pub fn move_next(self) -> bool { self.virt0::<"MoveNext", bool>() }
    pub fn get_current(self) -> System::Object { self.virt0::<"get_Current", System::Object>() }
    pub fn get_text_element(self) -> System::String { self.instance0::<"GetTextElement", System::String>() }
    pub fn get_element_index(self) -> i32 { self.instance0::<"get_ElementIndex", i32>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type TextInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.TextInfo">;
use super::super::*;
impl From<TextInfo> for System::Object {
 fn from(v:TextInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TextInfo>(v)
}} 
impl TextInfo {
    pub fn get_ansicode_page(self) -> i32 { self.instance0::<"get_ANSICodePage", i32>() }
    pub fn get_oemcode_page(self) -> i32 { self.instance0::<"get_OEMCodePage", i32>() }
    pub fn get_mac_code_page(self) -> i32 { self.instance0::<"get_MacCodePage", i32>() }
    pub fn get_ebcdiccode_page(self) -> i32 { self.instance0::<"get_EBCDICCodePage", i32>() }
    pub fn get_lcid(self) -> i32 { self.instance0::<"get_LCID", i32>() }
    pub fn get_culture_name(self) -> System::String { self.instance0::<"get_CultureName", System::String>() }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn read_only(a1: System::Globalization::TextInfo) -> System::Globalization::TextInfo { Self::static1::<"ReadOnly", System::Globalization::TextInfo, System::Globalization::TextInfo>(a1) }
    pub fn get_list_separator(self) -> System::String { self.instance0::<"get_ListSeparator", System::String>() }
    pub fn set_list_separator(self, a1: System::String) { self.instance1::<"set_ListSeparator", System::String, ()>(a1) }
    pub fn to_lower(self, a1: System::String) -> System::String { self.instance1::<"ToLower", System::String, System::String>(a1) }
    pub fn to_upper(self, a1: System::String) -> System::String { self.instance1::<"ToUpper", System::String, System::String>(a1) }
    pub fn get_is_right_to_left(self) -> bool { self.instance0::<"get_IsRightToLeft", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn to_title_case(self, a1: System::String) -> System::String { self.instance1::<"ToTitleCase", System::String, System::String>(a1) }
}
pub type ThaiBuddhistCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.ThaiBuddhistCalendar">;
use super::super::*;
impl From<ThaiBuddhistCalendar> for System::Globalization::Calendar {
 fn from(v:ThaiBuddhistCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,ThaiBuddhistCalendar>(v)
}} 
impl ThaiBuddhistCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UmAlQuraCalendar =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Globalization.UmAlQuraCalendar">;
use super::super::*;
impl From<UmAlQuraCalendar> for System::Globalization::Calendar {
 fn from(v:UmAlQuraCalendar)->System::Globalization::Calendar{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Globalization::Calendar,UmAlQuraCalendar>(v)
}} 
impl UmAlQuraCalendar {
    pub fn get_days_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetDaysInYear", i32, i32, i32>(a1, a2) }
    pub fn get_months_in_year(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetMonthsInYear", i32, i32, i32>(a1, a2) }
    pub fn get_leap_month(self, a1: i32, a2: i32) -> i32 { self.instance2::<"GetLeapMonth", i32, i32, i32>(a1, a2) }
    pub fn is_leap_year(self, a1: i32, a2: i32) -> bool { self.instance2::<"IsLeapYear", i32, i32, bool>(a1, a2) }
    pub fn get_two_digit_year_max(self) -> i32 { self.virt0::<"get_TwoDigitYearMax", i32>() }
    pub fn set_two_digit_year_max(self, a1: i32) { self.instance1::<"set_TwoDigitYearMax", i32, ()>(a1) }
    pub fn to_four_digit_year(self, a1: i32) -> i32 { self.instance1::<"ToFourDigitYear", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod Configuration{
pub mod Assemblies{
}
}
pub mod ComponentModel{
pub mod Design{
pub mod Serialization{
pub type DesignerSerializerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.Design.Serialization.DesignerSerializerAttribute">;
use super::super::super::super::*;
impl From<DesignerSerializerAttribute> for System::Attribute {
 fn from(v:DesignerSerializerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DesignerSerializerAttribute>(v)
}} 
impl DesignerSerializerAttribute {
    pub fn get_serializer_type_name(self) -> System::String { self.instance0::<"get_SerializerTypeName", System::String>() }
    pub fn get_serializer_base_type_name(self) -> System::String { self.instance0::<"get_SerializerBaseTypeName", System::String>() }
    pub fn get_type_id(self) -> System::Object { self.virt0::<"get_TypeId", System::Object>() }
    pub fn new(a1: System::Type, a2: System::Type) -> Self { Self::ctor2(a1, a2) }
}
}
}
pub type DefaultValueAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ComponentModel.DefaultValueAttribute">;
use super::super::*;
impl From<DefaultValueAttribute> for System::Attribute {
 fn from(v:DefaultValueAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultValueAttribute>(v)
}} 
impl DefaultValueAttribute {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new(a1: System::Type, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type EditorBrowsableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ComponentModel.EditorBrowsableAttribute">;
use super::super::*;
impl From<EditorBrowsableAttribute> for System::Attribute {
 fn from(v:EditorBrowsableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EditorBrowsableAttribute>(v)
}} 
impl EditorBrowsableAttribute {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Win32Exception =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ComponentModel.Win32Exception">;
use super::super::*;
impl From<Win32Exception> for System::Runtime::InteropServices::ExternalException {
 fn from(v:Win32Exception)->System::Runtime::InteropServices::ExternalException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::ExternalException,Win32Exception>(v)
}} 
impl Win32Exception {
    pub fn get_native_error_code(self) -> i32 { self.instance0::<"get_NativeErrorCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DataErrorsChangedEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.DataErrorsChangedEventArgs">;
use super::super::*;
impl From<DataErrorsChangedEventArgs> for System::EventArgs {
 fn from(v:DataErrorsChangedEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,DataErrorsChangedEventArgs>(v)
}} 
impl DataErrorsChangedEventArgs {
    pub fn get_property_name(self) -> System::String { self.virt0::<"get_PropertyName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type INotifyDataErrorInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.INotifyDataErrorInfo">;
use super::super::*;
impl INotifyDataErrorInfo {
    pub fn get_has_errors(self) -> bool { self.virt0::<"get_HasErrors", bool>() }
}
pub type INotifyPropertyChanged =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.INotifyPropertyChanged">;
use super::super::*;
pub type INotifyPropertyChanging =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.INotifyPropertyChanging">;
use super::super::*;
pub type PropertyChangedEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.PropertyChangedEventArgs">;
use super::super::*;
impl From<PropertyChangedEventArgs> for System::EventArgs {
 fn from(v:PropertyChangedEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,PropertyChangedEventArgs>(v)
}} 
impl PropertyChangedEventArgs {
    pub fn get_property_name(self) -> System::String { self.virt0::<"get_PropertyName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type PropertyChangedEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.PropertyChangedEventHandler">;
use super::super::*;
impl From<PropertyChangedEventHandler> for System::MulticastDelegate {
 fn from(v:PropertyChangedEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,PropertyChangedEventHandler>(v)
}} 
impl PropertyChangedEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::ComponentModel::PropertyChangedEventArgs) { self.instance2::<"Invoke", System::Object, System::ComponentModel::PropertyChangedEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type PropertyChangingEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.PropertyChangingEventArgs">;
use super::super::*;
impl From<PropertyChangingEventArgs> for System::EventArgs {
 fn from(v:PropertyChangingEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,PropertyChangingEventArgs>(v)
}} 
impl PropertyChangingEventArgs {
    pub fn get_property_name(self) -> System::String { self.virt0::<"get_PropertyName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type PropertyChangingEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.PropertyChangingEventHandler">;
use super::super::*;
impl From<PropertyChangingEventHandler> for System::MulticastDelegate {
 fn from(v:PropertyChangingEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,PropertyChangingEventHandler>(v)
}} 
impl PropertyChangingEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::ComponentModel::PropertyChangingEventArgs) { self.instance2::<"Invoke", System::Object, System::ComponentModel::PropertyChangingEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type TypeConverterAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.TypeConverterAttribute">;
use super::super::*;
impl From<TypeConverterAttribute> for System::Attribute {
 fn from(v:TypeConverterAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeConverterAttribute>(v)
}} 
impl TypeConverterAttribute {
    pub fn get_converter_type_name(self) -> System::String { self.instance0::<"get_ConverterTypeName", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TypeDescriptionProviderAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.ComponentModel.TypeDescriptionProviderAttribute">;
use super::super::*;
impl From<TypeDescriptionProviderAttribute> for System::Attribute {
 fn from(v:TypeDescriptionProviderAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeDescriptionProviderAttribute>(v)
}} 
impl TypeDescriptionProviderAttribute {
    pub fn get_type_name(self) -> System::String { self.instance0::<"get_TypeName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type CancelEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel","System.ComponentModel.CancelEventArgs">;
use super::super::*;
impl From<CancelEventArgs> for System::EventArgs {
 fn from(v:CancelEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,CancelEventArgs>(v)
}} 
impl CancelEventArgs {
    pub fn get_cancel(self) -> bool { self.instance0::<"get_Cancel", bool>() }
    pub fn set_cancel(self, a1: bool) { self.instance1::<"set_Cancel", bool, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IChangeTracking =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel","System.ComponentModel.IChangeTracking">;
use super::super::*;
impl IChangeTracking {
    pub fn get_is_changed(self) -> bool { self.virt0::<"get_IsChanged", bool>() }
    pub fn accept_changes(self) { self.virt0::<"AcceptChanges", ()>() }
}
pub type IEditableObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel","System.ComponentModel.IEditableObject">;
use super::super::*;
impl IEditableObject {
    pub fn begin_edit(self) { self.virt0::<"BeginEdit", ()>() }
    pub fn end_edit(self) { self.virt0::<"EndEdit", ()>() }
    pub fn cancel_edit(self) { self.virt0::<"CancelEdit", ()>() }
}
pub type IRevertibleChangeTracking =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel","System.ComponentModel.IRevertibleChangeTracking">;
use super::super::*;
impl IRevertibleChangeTracking {
    pub fn reject_changes(self) { self.virt0::<"RejectChanges", ()>() }
}
pub type ISynchronizeInvoke =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ISynchronizeInvoke">;
use super::super::*;
impl ISynchronizeInvoke {
    pub fn get_invoke_required(self) -> bool { self.virt0::<"get_InvokeRequired", bool>() }
}
pub type BrowsableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.BrowsableAttribute">;
use super::super::*;
impl From<BrowsableAttribute> for System::Attribute {
 fn from(v:BrowsableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,BrowsableAttribute>(v)
}} 
impl BrowsableAttribute {
    pub fn get_browsable(self) -> bool { self.instance0::<"get_Browsable", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type CategoryAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.CategoryAttribute">;
use super::super::*;
impl From<CategoryAttribute> for System::Attribute {
 fn from(v:CategoryAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CategoryAttribute>(v)
}} 
impl CategoryAttribute {
    pub fn get_action() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Action", System::ComponentModel::CategoryAttribute>() }
    pub fn get_appearance() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Appearance", System::ComponentModel::CategoryAttribute>() }
    pub fn get_asynchronous() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Asynchronous", System::ComponentModel::CategoryAttribute>() }
    pub fn get_behavior() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Behavior", System::ComponentModel::CategoryAttribute>() }
    pub fn get_data() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Data", System::ComponentModel::CategoryAttribute>() }
    pub fn get_default() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Default", System::ComponentModel::CategoryAttribute>() }
    pub fn get_design() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Design", System::ComponentModel::CategoryAttribute>() }
    pub fn get_drag_drop() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_DragDrop", System::ComponentModel::CategoryAttribute>() }
    pub fn get_focus() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Focus", System::ComponentModel::CategoryAttribute>() }
    pub fn get_format() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Format", System::ComponentModel::CategoryAttribute>() }
    pub fn get_key() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Key", System::ComponentModel::CategoryAttribute>() }
    pub fn get_layout() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Layout", System::ComponentModel::CategoryAttribute>() }
    pub fn get_mouse() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_Mouse", System::ComponentModel::CategoryAttribute>() }
    pub fn get_window_style() -> System::ComponentModel::CategoryAttribute { Self::static0::<"get_WindowStyle", System::ComponentModel::CategoryAttribute>() }
    pub fn get_category(self) -> System::String { self.instance0::<"get_Category", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Component =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.Component">;
use super::super::*;
impl From<Component> for System::MarshalByRefObject {
 fn from(v:Component)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,Component>(v)
}} 
impl Component {
    pub fn add_disposed(self, a1: System::EventHandler) { self.instance1::<"add_Disposed", System::EventHandler, ()>(a1) }
    pub fn remove_disposed(self, a1: System::EventHandler) { self.instance1::<"remove_Disposed", System::EventHandler, ()>(a1) }
    pub fn get_site(self) -> System::ComponentModel::ISite { self.virt0::<"get_Site", System::ComponentModel::ISite>() }
    pub fn set_site(self, a1: System::ComponentModel::ISite) { self.instance1::<"set_Site", System::ComponentModel::ISite, ()>(a1) }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn get_container(self) -> System::ComponentModel::IContainer { self.instance0::<"get_Container", System::ComponentModel::IContainer>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ComponentCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ComponentCollection">;
use super::super::*;
impl From<ComponentCollection> for System::Collections::ReadOnlyCollectionBase {
 fn from(v:ComponentCollection)->System::Collections::ReadOnlyCollectionBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Collections::ReadOnlyCollectionBase,ComponentCollection>(v)
}} 
impl ComponentCollection {
    pub fn get_item(self, a1: System::String) -> System::ComponentModel::IComponent { self.instance1::<"get_Item", System::String, System::ComponentModel::IComponent>(a1) }
}
pub type DescriptionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DescriptionAttribute">;
use super::super::*;
impl From<DescriptionAttribute> for System::Attribute {
 fn from(v:DescriptionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DescriptionAttribute>(v)
}} 
impl DescriptionAttribute {
    pub fn get_description(self) -> System::String { self.virt0::<"get_Description", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DesignerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DesignerAttribute">;
use super::super::*;
impl From<DesignerAttribute> for System::Attribute {
 fn from(v:DesignerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DesignerAttribute>(v)
}} 
impl DesignerAttribute {
    pub fn get_designer_base_type_name(self) -> System::String { self.instance0::<"get_DesignerBaseTypeName", System::String>() }
    pub fn get_designer_type_name(self) -> System::String { self.instance0::<"get_DesignerTypeName", System::String>() }
    pub fn get_type_id(self) -> System::Object { self.virt0::<"get_TypeId", System::Object>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type DesignerCategoryAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DesignerCategoryAttribute">;
use super::super::*;
impl From<DesignerCategoryAttribute> for System::Attribute {
 fn from(v:DesignerCategoryAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DesignerCategoryAttribute>(v)
}} 
impl DesignerCategoryAttribute {
    pub fn get_category(self) -> System::String { self.instance0::<"get_Category", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn get_type_id(self) -> System::Object { self.virt0::<"get_TypeId", System::Object>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DesignerSerializationVisibilityAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DesignerSerializationVisibilityAttribute">;
use super::super::*;
impl From<DesignerSerializationVisibilityAttribute> for System::Attribute {
 fn from(v:DesignerSerializationVisibilityAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DesignerSerializationVisibilityAttribute>(v)
}} 
impl DesignerSerializationVisibilityAttribute {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
}
pub type DesignOnlyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DesignOnlyAttribute">;
use super::super::*;
impl From<DesignOnlyAttribute> for System::Attribute {
 fn from(v:DesignOnlyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DesignOnlyAttribute>(v)
}} 
impl DesignOnlyAttribute {
    pub fn get_is_design_only(self) -> bool { self.instance0::<"get_IsDesignOnly", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type DisplayNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.DisplayNameAttribute">;
use super::super::*;
impl From<DisplayNameAttribute> for System::Attribute {
 fn from(v:DisplayNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DisplayNameAttribute>(v)
}} 
impl DisplayNameAttribute {
    pub fn get_display_name(self) -> System::String { self.virt0::<"get_DisplayName", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EditorAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.EditorAttribute">;
use super::super::*;
impl From<EditorAttribute> for System::Attribute {
 fn from(v:EditorAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EditorAttribute>(v)
}} 
impl EditorAttribute {
    pub fn get_editor_base_type_name(self) -> System::String { self.instance0::<"get_EditorBaseTypeName", System::String>() }
    pub fn get_editor_type_name(self) -> System::String { self.instance0::<"get_EditorTypeName", System::String>() }
    pub fn get_type_id(self) -> System::Object { self.virt0::<"get_TypeId", System::Object>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventHandlerList =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.EventHandlerList">;
use super::super::*;
impl From<EventHandlerList> for System::Object {
 fn from(v:EventHandlerList)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EventHandlerList>(v)
}} 
impl EventHandlerList {
    pub fn get_item(self, a1: System::Object) -> System::Delegate { self.instance1::<"get_Item", System::Object, System::Delegate>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"set_Item", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn add_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"AddHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn add_handlers(self, a1: System::ComponentModel::EventHandlerList) { self.instance1::<"AddHandlers", System::ComponentModel::EventHandlerList, ()>(a1) }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn remove_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"RemoveHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IComponent =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.IComponent">;
use super::super::*;
impl IComponent {
    pub fn get_site(self) -> System::ComponentModel::ISite { self.virt0::<"get_Site", System::ComponentModel::ISite>() }
}
pub type IContainer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.IContainer">;
use super::super::*;
impl IContainer {
    pub fn get_components(self) -> System::ComponentModel::ComponentCollection { self.virt0::<"get_Components", System::ComponentModel::ComponentCollection>() }
}
pub type ImmutableObjectAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ImmutableObjectAttribute">;
use super::super::*;
impl From<ImmutableObjectAttribute> for System::Attribute {
 fn from(v:ImmutableObjectAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ImmutableObjectAttribute>(v)
}} 
impl ImmutableObjectAttribute {
    pub fn get_immutable(self) -> bool { self.instance0::<"get_Immutable", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type InitializationEventAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.InitializationEventAttribute">;
use super::super::*;
impl From<InitializationEventAttribute> for System::Attribute {
 fn from(v:InitializationEventAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InitializationEventAttribute>(v)
}} 
impl InitializationEventAttribute {
    pub fn get_event_name(self) -> System::String { self.instance0::<"get_EventName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type InvalidAsynchronousStateException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.InvalidAsynchronousStateException">;
use super::super::*;
impl From<InvalidAsynchronousStateException> for System::ArgumentException {
 fn from(v:InvalidAsynchronousStateException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,InvalidAsynchronousStateException>(v)
}} 
impl InvalidAsynchronousStateException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidEnumArgumentException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.InvalidEnumArgumentException">;
use super::super::*;
impl From<InvalidEnumArgumentException> for System::ArgumentException {
 fn from(v:InvalidEnumArgumentException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,InvalidEnumArgumentException>(v)
}} 
impl InvalidEnumArgumentException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ISite =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ISite">;
use super::super::*;
impl ISite {
    pub fn get_component(self) -> System::ComponentModel::IComponent { self.virt0::<"get_Component", System::ComponentModel::IComponent>() }
    pub fn get_container(self) -> System::ComponentModel::IContainer { self.virt0::<"get_Container", System::ComponentModel::IContainer>() }
    pub fn get_design_mode(self) -> bool { self.virt0::<"get_DesignMode", bool>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
}
pub type ISupportInitialize =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ISupportInitialize">;
use super::super::*;
impl ISupportInitialize {
    pub fn begin_init(self) { self.virt0::<"BeginInit", ()>() }
    pub fn end_init(self) { self.virt0::<"EndInit", ()>() }
}
pub type LocalizableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.LocalizableAttribute">;
use super::super::*;
impl From<LocalizableAttribute> for System::Attribute {
 fn from(v:LocalizableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,LocalizableAttribute>(v)
}} 
impl LocalizableAttribute {
    pub fn get_is_localizable(self) -> bool { self.instance0::<"get_IsLocalizable", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type MergablePropertyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.MergablePropertyAttribute">;
use super::super::*;
impl From<MergablePropertyAttribute> for System::Attribute {
 fn from(v:MergablePropertyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MergablePropertyAttribute>(v)
}} 
impl MergablePropertyAttribute {
    pub fn get_allow_merge(self) -> bool { self.instance0::<"get_AllowMerge", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type NotifyParentPropertyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.NotifyParentPropertyAttribute">;
use super::super::*;
impl From<NotifyParentPropertyAttribute> for System::Attribute {
 fn from(v:NotifyParentPropertyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NotifyParentPropertyAttribute>(v)
}} 
impl NotifyParentPropertyAttribute {
    pub fn get_notify_parent(self) -> bool { self.instance0::<"get_NotifyParent", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ParenthesizePropertyNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ParenthesizePropertyNameAttribute">;
use super::super::*;
impl From<ParenthesizePropertyNameAttribute> for System::Attribute {
 fn from(v:ParenthesizePropertyNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ParenthesizePropertyNameAttribute>(v)
}} 
impl ParenthesizePropertyNameAttribute {
    pub fn get_need_parenthesis(self) -> bool { self.instance0::<"get_NeedParenthesis", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ReadOnlyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.ReadOnlyAttribute">;
use super::super::*;
impl From<ReadOnlyAttribute> for System::Attribute {
 fn from(v:ReadOnlyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ReadOnlyAttribute>(v)
}} 
impl ReadOnlyAttribute {
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type RefreshPropertiesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel.Primitives","System.ComponentModel.RefreshPropertiesAttribute">;
use super::super::*;
impl From<RefreshPropertiesAttribute> for System::Attribute {
 fn from(v:RefreshPropertiesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RefreshPropertiesAttribute>(v)
}} 
impl RefreshPropertiesAttribute {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
}
}
pub mod CodeDom{
pub mod Compiler{
pub type GeneratedCodeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CodeDom.Compiler.GeneratedCodeAttribute">;
use super::super::super::*;
impl From<GeneratedCodeAttribute> for System::Attribute {
 fn from(v:GeneratedCodeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,GeneratedCodeAttribute>(v)
}} 
impl GeneratedCodeAttribute {
    pub fn get_tool(self) -> System::String { self.instance0::<"get_Tool", System::String>() }
    pub fn get_version(self) -> System::String { self.instance0::<"get_Version", System::String>() }
    pub fn new(a1: System::String, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type IndentedTextWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CodeDom.Compiler.IndentedTextWriter">;
use super::super::super::*;
impl From<IndentedTextWriter> for System::IO::TextWriter {
 fn from(v:IndentedTextWriter)->System::IO::TextWriter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::TextWriter,IndentedTextWriter>(v)
}} 
impl IndentedTextWriter {
    pub fn get_encoding(self) -> System::Text::Encoding { self.virt0::<"get_Encoding", System::Text::Encoding>() }
    pub fn get_new_line(self) -> System::String { self.virt0::<"get_NewLine", System::String>() }
    pub fn set_new_line(self, a1: System::String) { self.instance1::<"set_NewLine", System::String, ()>(a1) }
    pub fn get_indent(self) -> i32 { self.instance0::<"get_Indent", i32>() }
    pub fn set_indent(self, a1: i32) { self.instance1::<"set_Indent", i32, ()>(a1) }
    pub fn get_inner_writer(self) -> System::IO::TextWriter { self.instance0::<"get_InnerWriter", System::IO::TextWriter>() }
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn flush_async(self) -> System::Threading::Tasks::Task { self.virt0::<"FlushAsync", System::Threading::Tasks::Task>() }
    pub fn write(self, a1: System::String) { self.instance1::<"Write", System::String, ()>(a1) }
    pub fn write_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn write_line_no_tabs(self, a1: System::String) { self.instance1::<"WriteLineNoTabs", System::String, ()>(a1) }
    pub fn write_line_no_tabs_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteLineNoTabsAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn write_line(self, a1: System::String) { self.instance1::<"WriteLine", System::String, ()>(a1) }
    pub fn write_line_async(self) -> System::Threading::Tasks::Task { self.virt0::<"WriteLineAsync", System::Threading::Tasks::Task>() }
    pub fn new(a1: System::IO::TextWriter) -> Self { Self::ctor1(a1) }
}
}
}
pub mod Buffers{
pub mod Text{
pub type Base64 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.Text.Base64">;
use super::super::super::*;
impl From<Base64> for System::Object {
 fn from(v:Base64)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Base64>(v)
}} 
impl Base64 {
    pub fn get_max_encoded_to_utf8_length(a1: i32) -> i32 { Self::static1::<"GetMaxEncodedToUtf8Length", i32, i32>(a1) }
    pub fn get_max_decoded_from_utf8_length(a1: i32) -> i32 { Self::static1::<"GetMaxDecodedFromUtf8Length", i32, i32>(a1) }
}
pub type Utf8Formatter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.Text.Utf8Formatter">;
use super::super::super::*;
impl From<Utf8Formatter> for System::Object {
 fn from(v:Utf8Formatter)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Utf8Formatter>(v)
}} 
pub type Utf8Parser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.Text.Utf8Parser">;
use super::super::super::*;
impl From<Utf8Parser> for System::Object {
 fn from(v:Utf8Parser)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Utf8Parser>(v)
}} 
}
pub mod Binary{
pub type BinaryPrimitives =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.Binary.BinaryPrimitives">;
use super::super::super::*;
impl From<BinaryPrimitives> for System::Object {
 fn from(v:BinaryPrimitives)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BinaryPrimitives>(v)
}} 
impl BinaryPrimitives {
    pub fn reverse_endianness(a1: i8) -> i8 { Self::static1::<"ReverseEndianness", i8, i8>(a1) }
}
}
pub type IPinnable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.IPinnable">;
use super::super::*;
impl IPinnable {
    pub fn unpin(self) { self.virt0::<"Unpin", ()>() }
}
pub type SearchValues =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffers.SearchValues">;
use super::super::*;
impl From<SearchValues> for System::Object {
 fn from(v:SearchValues)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SearchValues>(v)
}} 
pub type BuffersExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Memory","System.Buffers.BuffersExtensions">;
use super::super::*;
impl From<BuffersExtensions> for System::Object {
 fn from(v:BuffersExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BuffersExtensions>(v)
}} 
pub type SequenceReaderExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Memory","System.Buffers.SequenceReaderExtensions">;
use super::super::*;
impl From<SequenceReaderExtensions> for System::Object {
 fn from(v:SequenceReaderExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SequenceReaderExtensions>(v)
}} 
}
pub mod Threading{
pub mod Tasks{
pub mod Sources{
pub type IValueTaskSource =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.Sources.IValueTaskSource">;
use super::super::super::super::*;
}
pub type ConcurrentExclusiveSchedulerPair =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.ConcurrentExclusiveSchedulerPair">;
use super::super::super::*;
impl From<ConcurrentExclusiveSchedulerPair> for System::Object {
 fn from(v:ConcurrentExclusiveSchedulerPair)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ConcurrentExclusiveSchedulerPair>(v)
}} 
impl ConcurrentExclusiveSchedulerPair {
    pub fn complete(self) { self.instance0::<"Complete", ()>() }
    pub fn get_completion(self) -> System::Threading::Tasks::Task { self.instance0::<"get_Completion", System::Threading::Tasks::Task>() }
    pub fn get_concurrent_scheduler(self) -> System::Threading::Tasks::TaskScheduler { self.instance0::<"get_ConcurrentScheduler", System::Threading::Tasks::TaskScheduler>() }
    pub fn get_exclusive_scheduler(self) -> System::Threading::Tasks::TaskScheduler { self.instance0::<"get_ExclusiveScheduler", System::Threading::Tasks::TaskScheduler>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Task =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.Task">;
use super::super::super::*;
impl From<Task> for System::Object {
 fn from(v:Task)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Task>(v)
}} 
impl Task {
    pub fn start(self) { self.instance0::<"Start", ()>() }
    pub fn run_synchronously(self) { self.instance0::<"RunSynchronously", ()>() }
    pub fn get_id(self) -> i32 { self.instance0::<"get_Id", i32>() }
    pub fn get_exception(self) -> System::AggregateException { self.instance0::<"get_Exception", System::AggregateException>() }
    pub fn get_is_canceled(self) -> bool { self.instance0::<"get_IsCanceled", bool>() }
    pub fn get_is_completed(self) -> bool { self.virt0::<"get_IsCompleted", bool>() }
    pub fn get_is_completed_successfully(self) -> bool { self.instance0::<"get_IsCompletedSuccessfully", bool>() }
    pub fn get_async_state(self) -> System::Object { self.virt0::<"get_AsyncState", System::Object>() }
    pub fn get_factory() -> System::Threading::Tasks::TaskFactory { Self::static0::<"get_Factory", System::Threading::Tasks::TaskFactory>() }
    pub fn get_completed_task() -> System::Threading::Tasks::Task { Self::static0::<"get_CompletedTask", System::Threading::Tasks::Task>() }
    pub fn get_is_faulted(self) -> bool { self.instance0::<"get_IsFaulted", bool>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn wait(self) { self.instance0::<"Wait", ()>() }
    pub fn from_exception(a1: System::Exception) -> System::Threading::Tasks::Task { Self::static1::<"FromException", System::Exception, System::Threading::Tasks::Task>(a1) }
    pub fn run(a1: System::Action) -> System::Threading::Tasks::Task { Self::static1::<"Run", System::Action, System::Threading::Tasks::Task>(a1) }
    pub fn delay(a1: i32) -> System::Threading::Tasks::Task { Self::static1::<"Delay", i32, System::Threading::Tasks::Task>(a1) }
    pub fn new(a1: System::Action) -> Self { Self::ctor1(a1) }
}
pub type TaskAsyncEnumerableExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskAsyncEnumerableExtensions">;
use super::super::super::*;
impl From<TaskAsyncEnumerableExtensions> for System::Object {
 fn from(v:TaskAsyncEnumerableExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskAsyncEnumerableExtensions>(v)
}} 
pub type TaskCanceledException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskCanceledException">;
use super::super::super::*;
impl From<TaskCanceledException> for System::OperationCanceledException {
 fn from(v:TaskCanceledException)->System::OperationCanceledException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::OperationCanceledException,TaskCanceledException>(v)
}} 
impl TaskCanceledException {
    pub fn get_task(self) -> System::Threading::Tasks::Task { self.instance0::<"get_Task", System::Threading::Tasks::Task>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaskCompletionSource =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskCompletionSource">;
use super::super::super::*;
impl From<TaskCompletionSource> for System::Object {
 fn from(v:TaskCompletionSource)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskCompletionSource>(v)
}} 
impl TaskCompletionSource {
    pub fn get_task(self) -> System::Threading::Tasks::Task { self.instance0::<"get_Task", System::Threading::Tasks::Task>() }
    pub fn set_exception(self, a1: System::Exception) { self.instance1::<"SetException", System::Exception, ()>(a1) }
    pub fn try_set_exception(self, a1: System::Exception) -> bool { self.instance1::<"TrySetException", System::Exception, bool>(a1) }
    pub fn set_result(self) { self.instance0::<"SetResult", ()>() }
    pub fn try_set_result(self) -> bool { self.instance0::<"TrySetResult", bool>() }
    pub fn set_canceled(self) { self.instance0::<"SetCanceled", ()>() }
    pub fn try_set_canceled(self) -> bool { self.instance0::<"TrySetCanceled", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaskExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskExtensions">;
use super::super::super::*;
impl From<TaskExtensions> for System::Object {
 fn from(v:TaskExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskExtensions>(v)
}} 
pub type TaskFactory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskFactory">;
use super::super::super::*;
impl From<TaskFactory> for System::Object {
 fn from(v:TaskFactory)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskFactory>(v)
}} 
impl TaskFactory {
    pub fn get_scheduler(self) -> System::Threading::Tasks::TaskScheduler { self.instance0::<"get_Scheduler", System::Threading::Tasks::TaskScheduler>() }
    pub fn start_new(self, a1: System::Action) -> System::Threading::Tasks::Task { self.instance1::<"StartNew", System::Action, System::Threading::Tasks::Task>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaskScheduler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskScheduler">;
use super::super::super::*;
impl From<TaskScheduler> for System::Object {
 fn from(v:TaskScheduler)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskScheduler>(v)
}} 
impl TaskScheduler {
    pub fn get_maximum_concurrency_level(self) -> i32 { self.virt0::<"get_MaximumConcurrencyLevel", i32>() }
    pub fn get_default() -> System::Threading::Tasks::TaskScheduler { Self::static0::<"get_Default", System::Threading::Tasks::TaskScheduler>() }
    pub fn get_current() -> System::Threading::Tasks::TaskScheduler { Self::static0::<"get_Current", System::Threading::Tasks::TaskScheduler>() }
    pub fn from_current_synchronization_context() -> System::Threading::Tasks::TaskScheduler { Self::static0::<"FromCurrentSynchronizationContext", System::Threading::Tasks::TaskScheduler>() }
    pub fn get_id(self) -> i32 { self.instance0::<"get_Id", i32>() }
}
pub type UnobservedTaskExceptionEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.UnobservedTaskExceptionEventArgs">;
use super::super::super::*;
impl From<UnobservedTaskExceptionEventArgs> for System::EventArgs {
 fn from(v:UnobservedTaskExceptionEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,UnobservedTaskExceptionEventArgs>(v)
}} 
impl UnobservedTaskExceptionEventArgs {
    pub fn set_observed(self) { self.instance0::<"SetObserved", ()>() }
    pub fn get_observed(self) -> bool { self.instance0::<"get_Observed", bool>() }
    pub fn get_exception(self) -> System::AggregateException { self.instance0::<"get_Exception", System::AggregateException>() }
    pub fn new(a1: System::AggregateException) -> Self { Self::ctor1(a1) }
}
pub type TaskSchedulerException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskSchedulerException">;
use super::super::super::*;
impl From<TaskSchedulerException> for System::Exception {
 fn from(v:TaskSchedulerException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,TaskSchedulerException>(v)
}} 
impl TaskSchedulerException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TaskToAsyncResult =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Tasks.TaskToAsyncResult">;
use super::super::super::*;
impl From<TaskToAsyncResult> for System::Object {
 fn from(v:TaskToAsyncResult)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TaskToAsyncResult>(v)
}} 
impl TaskToAsyncResult {
    pub fn end(a1: System::IAsyncResult) { Self::static1::<"End", System::IAsyncResult, ()>(a1) }
    pub fn unwrap(a1: System::IAsyncResult) -> System::Threading::Tasks::Task { Self::static1::<"Unwrap", System::IAsyncResult, System::Threading::Tasks::Task>(a1) }
}
}
pub type Interlocked =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Interlocked">;
use super::super::*;
impl From<Interlocked> for System::Object {
 fn from(v:Interlocked)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Interlocked>(v)
}} 
impl Interlocked {
    pub fn memory_barrier_process_wide() { Self::static0::<"MemoryBarrierProcessWide", ()>() }
    pub fn memory_barrier() { Self::static0::<"MemoryBarrier", ()>() }
}
pub type Monitor =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Monitor">;
use super::super::*;
impl From<Monitor> for System::Object {
 fn from(v:Monitor)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Monitor>(v)
}} 
impl Monitor {
    pub fn enter(a1: System::Object) { Self::static1::<"Enter", System::Object, ()>(a1) }
    pub fn exit(a1: System::Object) { Self::static1::<"Exit", System::Object, ()>(a1) }
    pub fn try_enter(a1: System::Object) -> bool { Self::static1::<"TryEnter", System::Object, bool>(a1) }
    pub fn is_entered(a1: System::Object) -> bool { Self::static1::<"IsEntered", System::Object, bool>(a1) }
    pub fn wait(a1: System::Object, a2: i32) -> bool { Self::static2::<"Wait", System::Object, i32, bool>(a1, a2) }
    pub fn pulse(a1: System::Object) { Self::static1::<"Pulse", System::Object, ()>(a1) }
    pub fn pulse_all(a1: System::Object) { Self::static1::<"PulseAll", System::Object, ()>(a1) }
    pub fn get_lock_contention_count() -> i64 { Self::static0::<"get_LockContentionCount", i64>() }
}
pub type SynchronizationContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.SynchronizationContext">;
use super::super::*;
impl From<SynchronizationContext> for System::Object {
 fn from(v:SynchronizationContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SynchronizationContext>(v)
}} 
impl SynchronizationContext {
    pub fn get_current() -> System::Threading::SynchronizationContext { Self::static0::<"get_Current", System::Threading::SynchronizationContext>() }
    pub fn is_wait_notification_required(self) -> bool { self.instance0::<"IsWaitNotificationRequired", bool>() }
    pub fn send(self, a1: System::Threading::SendOrPostCallback, a2: System::Object) { self.instance2::<"Send", System::Threading::SendOrPostCallback, System::Object, ()>(a1, a2) }
    pub fn post(self, a1: System::Threading::SendOrPostCallback, a2: System::Object) { self.instance2::<"Post", System::Threading::SendOrPostCallback, System::Object, ()>(a1, a2) }
    pub fn operation_started(self) { self.virt0::<"OperationStarted", ()>() }
    pub fn operation_completed(self) { self.virt0::<"OperationCompleted", ()>() }
    pub fn set_synchronization_context(a1: System::Threading::SynchronizationContext) { Self::static1::<"SetSynchronizationContext", System::Threading::SynchronizationContext, ()>(a1) }
    pub fn create_copy(self) -> System::Threading::SynchronizationContext { self.virt0::<"CreateCopy", System::Threading::SynchronizationContext>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Thread =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Thread">;
use super::super::*;
impl From<Thread> for System::Runtime::ConstrainedExecution::CriticalFinalizerObject {
 fn from(v:Thread)->System::Runtime::ConstrainedExecution::CriticalFinalizerObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::ConstrainedExecution::CriticalFinalizerObject,Thread>(v)
}} 
impl Thread {
    pub fn get_managed_thread_id(self) -> i32 { self.instance0::<"get_ManagedThreadId", i32>() }
    pub fn spin_wait(a1: i32) { Self::static1::<"SpinWait", i32, ()>(a1) }
    pub fn r#yield() -> bool { Self::static0::<"Yield", bool>() }
    pub fn get_is_alive(self) -> bool { self.instance0::<"get_IsAlive", bool>() }
    pub fn get_is_background(self) -> bool { self.instance0::<"get_IsBackground", bool>() }
    pub fn set_is_background(self, a1: bool) { self.instance1::<"set_IsBackground", bool, ()>(a1) }
    pub fn get_is_thread_pool_thread(self) -> bool { self.instance0::<"get_IsThreadPoolThread", bool>() }
    pub fn disable_com_object_eager_cleanup(self) { self.instance0::<"DisableComObjectEagerCleanup", ()>() }
    pub fn interrupt(self) { self.instance0::<"Interrupt", ()>() }
    pub fn join(self, a1: i32) -> bool { self.instance1::<"Join", i32, bool>(a1) }
    pub fn start(self, a1: System::Object) { self.instance1::<"Start", System::Object, ()>(a1) }
    pub fn unsafe_start(self, a1: System::Object) { self.instance1::<"UnsafeStart", System::Object, ()>(a1) }
    pub fn get_current_culture(self) -> System::Globalization::CultureInfo { self.instance0::<"get_CurrentCulture", System::Globalization::CultureInfo>() }
    pub fn set_current_culture(self, a1: System::Globalization::CultureInfo) { self.instance1::<"set_CurrentCulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_current_uiculture(self) -> System::Globalization::CultureInfo { self.instance0::<"get_CurrentUICulture", System::Globalization::CultureInfo>() }
    pub fn set_current_uiculture(self, a1: System::Globalization::CultureInfo) { self.instance1::<"set_CurrentUICulture", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_current_principal() -> System::Security::Principal::IPrincipal { Self::static0::<"get_CurrentPrincipal", System::Security::Principal::IPrincipal>() }
    pub fn set_current_principal(a1: System::Security::Principal::IPrincipal) { Self::static1::<"set_CurrentPrincipal", System::Security::Principal::IPrincipal, ()>(a1) }
    pub fn get_current_thread() -> System::Threading::Thread { Self::static0::<"get_CurrentThread", System::Threading::Thread>() }
    pub fn sleep(a1: i32) { Self::static1::<"Sleep", i32, ()>(a1) }
    pub fn get_execution_context(self) -> System::Threading::ExecutionContext { self.instance0::<"get_ExecutionContext", System::Threading::ExecutionContext>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn abort(self) { self.instance0::<"Abort", ()>() }
    pub fn reset_abort() { Self::static0::<"ResetAbort", ()>() }
    pub fn suspend(self) { self.instance0::<"Suspend", ()>() }
    pub fn resume(self) { self.instance0::<"Resume", ()>() }
    pub fn begin_critical_region() { Self::static0::<"BeginCriticalRegion", ()>() }
    pub fn end_critical_region() { Self::static0::<"EndCriticalRegion", ()>() }
    pub fn begin_thread_affinity() { Self::static0::<"BeginThreadAffinity", ()>() }
    pub fn end_thread_affinity() { Self::static0::<"EndThreadAffinity", ()>() }
    pub fn allocate_data_slot() -> System::LocalDataStoreSlot { Self::static0::<"AllocateDataSlot", System::LocalDataStoreSlot>() }
    pub fn allocate_named_data_slot(a1: System::String) -> System::LocalDataStoreSlot { Self::static1::<"AllocateNamedDataSlot", System::String, System::LocalDataStoreSlot>(a1) }
    pub fn get_named_data_slot(a1: System::String) -> System::LocalDataStoreSlot { Self::static1::<"GetNamedDataSlot", System::String, System::LocalDataStoreSlot>(a1) }
    pub fn free_named_data_slot(a1: System::String) { Self::static1::<"FreeNamedDataSlot", System::String, ()>(a1) }
    pub fn get_data(a1: System::LocalDataStoreSlot) -> System::Object { Self::static1::<"GetData", System::LocalDataStoreSlot, System::Object>(a1) }
    pub fn set_data(a1: System::LocalDataStoreSlot, a2: System::Object) { Self::static2::<"SetData", System::LocalDataStoreSlot, System::Object, ()>(a1, a2) }
    pub fn get_compressed_stack(self) -> System::Threading::CompressedStack { self.instance0::<"GetCompressedStack", System::Threading::CompressedStack>() }
    pub fn set_compressed_stack(self, a1: System::Threading::CompressedStack) { self.instance1::<"SetCompressedStack", System::Threading::CompressedStack, ()>(a1) }
    pub fn get_domain() -> System::AppDomain { Self::static0::<"GetDomain", System::AppDomain>() }
    pub fn get_domain_id() -> i32 { Self::static0::<"GetDomainID", i32>() }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn memory_barrier() { Self::static0::<"MemoryBarrier", ()>() }
    pub fn get_current_processor_id() -> i32 { Self::static0::<"GetCurrentProcessorId", i32>() }
    pub fn new(a1: System::Threading::ThreadStart) -> Self { Self::ctor1(a1) }
}
pub type ThreadPool =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadPool">;
use super::super::*;
impl From<ThreadPool> for System::Object {
 fn from(v:ThreadPool)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ThreadPool>(v)
}} 
impl ThreadPool {
    pub fn queue_user_work_item(a1: System::Threading::WaitCallback) -> bool { Self::static1::<"QueueUserWorkItem", System::Threading::WaitCallback, bool>(a1) }
    pub fn unsafe_queue_user_work_item(a1: System::Threading::WaitCallback, a2: System::Object) -> bool { Self::static2::<"UnsafeQueueUserWorkItem", System::Threading::WaitCallback, System::Object, bool>(a1, a2) }
    pub fn get_pending_work_item_count() -> i64 { Self::static0::<"get_PendingWorkItemCount", i64>() }
    pub fn set_max_threads(a1: i32, a2: i32) -> bool { Self::static2::<"SetMaxThreads", i32, i32, bool>(a1, a2) }
    pub fn set_min_threads(a1: i32, a2: i32) -> bool { Self::static2::<"SetMinThreads", i32, i32, bool>(a1, a2) }
    pub fn bind_handle(a1: isize) -> bool { Self::static1::<"BindHandle", isize, bool>(a1) }
    pub fn get_thread_count() -> i32 { Self::static0::<"get_ThreadCount", i32>() }
    pub fn get_completed_work_item_count() -> i64 { Self::static0::<"get_CompletedWorkItemCount", i64>() }
}
pub type WaitHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.WaitHandle">;
use super::super::*;
impl From<WaitHandle> for System::MarshalByRefObject {
 fn from(v:WaitHandle)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,WaitHandle>(v)
}} 
impl WaitHandle {
    pub fn get_handle(self) -> isize { self.virt0::<"get_Handle", isize>() }
    pub fn set_handle(self, a1: isize) { self.instance1::<"set_Handle", isize, ()>(a1) }
    pub fn get_safe_wait_handle(self) -> Microsoft::Win32::SafeHandles::SafeWaitHandle { self.instance0::<"get_SafeWaitHandle", Microsoft::Win32::SafeHandles::SafeWaitHandle>() }
    pub fn set_safe_wait_handle(self, a1: Microsoft::Win32::SafeHandles::SafeWaitHandle) { self.instance1::<"set_SafeWaitHandle", Microsoft::Win32::SafeHandles::SafeWaitHandle, ()>(a1) }
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn wait_one(self, a1: i32) -> bool { self.instance1::<"WaitOne", i32, bool>(a1) }
    pub fn signal_and_wait(a1: System::Threading::WaitHandle, a2: System::Threading::WaitHandle) -> bool { Self::static2::<"SignalAndWait", System::Threading::WaitHandle, System::Threading::WaitHandle, bool>(a1, a2) }
}
pub type AbandonedMutexException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.AbandonedMutexException">;
use super::super::*;
impl From<AbandonedMutexException> for System::SystemException {
 fn from(v:AbandonedMutexException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,AbandonedMutexException>(v)
}} 
impl AbandonedMutexException {
    pub fn get_mutex(self) -> System::Threading::Mutex { self.instance0::<"get_Mutex", System::Threading::Mutex>() }
    pub fn get_mutex_index(self) -> i32 { self.instance0::<"get_MutexIndex", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type AutoResetEvent =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.AutoResetEvent">;
use super::super::*;
impl From<AutoResetEvent> for System::Threading::EventWaitHandle {
 fn from(v:AutoResetEvent)->System::Threading::EventWaitHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Threading::EventWaitHandle,AutoResetEvent>(v)
}} 
impl AutoResetEvent {
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type CancellationTokenSource =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.CancellationTokenSource">;
use super::super::*;
impl From<CancellationTokenSource> for System::Object {
 fn from(v:CancellationTokenSource)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CancellationTokenSource>(v)
}} 
impl CancellationTokenSource {
    pub fn get_is_cancellation_requested(self) -> bool { self.instance0::<"get_IsCancellationRequested", bool>() }
    pub fn cancel(self) { self.instance0::<"Cancel", ()>() }
    pub fn cancel_async(self) -> System::Threading::Tasks::Task { self.instance0::<"CancelAsync", System::Threading::Tasks::Task>() }
    pub fn cancel_after(self, a1: i32) { self.instance1::<"CancelAfter", i32, ()>(a1) }
    pub fn try_reset(self) -> bool { self.instance0::<"TryReset", bool>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type CompressedStack =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.CompressedStack">;
use super::super::*;
impl From<CompressedStack> for System::Object {
 fn from(v:CompressedStack)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CompressedStack>(v)
}} 
impl CompressedStack {
    pub fn capture() -> System::Threading::CompressedStack { Self::static0::<"Capture", System::Threading::CompressedStack>() }
    pub fn create_copy(self) -> System::Threading::CompressedStack { self.instance0::<"CreateCopy", System::Threading::CompressedStack>() }
    pub fn get_compressed_stack() -> System::Threading::CompressedStack { Self::static0::<"GetCompressedStack", System::Threading::CompressedStack>() }
}
pub type EventWaitHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.EventWaitHandle">;
use super::super::*;
impl From<EventWaitHandle> for System::Threading::WaitHandle {
 fn from(v:EventWaitHandle)->System::Threading::WaitHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Threading::WaitHandle,EventWaitHandle>(v)
}} 
impl EventWaitHandle {
    pub fn open_existing(a1: System::String) -> System::Threading::EventWaitHandle { Self::static1::<"OpenExisting", System::String, System::Threading::EventWaitHandle>(a1) }
    pub fn reset(self) -> bool { self.instance0::<"Reset", bool>() }
    pub fn set(self) -> bool { self.instance0::<"Set", bool>() }
}
pub type ContextCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ContextCallback">;
use super::super::*;
impl From<ContextCallback> for System::MulticastDelegate {
 fn from(v:ContextCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ContextCallback>(v)
}} 
impl ContextCallback {
    pub fn invoke(self, a1: System::Object) { self.instance1::<"Invoke", System::Object, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ExecutionContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ExecutionContext">;
use super::super::*;
impl From<ExecutionContext> for System::Object {
 fn from(v:ExecutionContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExecutionContext>(v)
}} 
impl ExecutionContext {
    pub fn capture() -> System::Threading::ExecutionContext { Self::static0::<"Capture", System::Threading::ExecutionContext>() }
    pub fn restore_flow() { Self::static0::<"RestoreFlow", ()>() }
    pub fn is_flow_suppressed() -> bool { Self::static0::<"IsFlowSuppressed", bool>() }
    pub fn restore(a1: System::Threading::ExecutionContext) { Self::static1::<"Restore", System::Threading::ExecutionContext, ()>(a1) }
    pub fn create_copy(self) -> System::Threading::ExecutionContext { self.instance0::<"CreateCopy", System::Threading::ExecutionContext>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
}
pub type IOCompletionCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.IOCompletionCallback">;
use super::super::*;
impl From<IOCompletionCallback> for System::MulticastDelegate {
 fn from(v:IOCompletionCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,IOCompletionCallback>(v)
}} 
impl IOCompletionCallback {
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type IThreadPoolWorkItem =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.IThreadPoolWorkItem">;
use super::super::*;
impl IThreadPoolWorkItem {
    pub fn execute(self) { self.virt0::<"Execute", ()>() }
}
pub type LazyInitializer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.LazyInitializer">;
use super::super::*;
impl From<LazyInitializer> for System::Object {
 fn from(v:LazyInitializer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,LazyInitializer>(v)
}} 
pub type LockRecursionException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.LockRecursionException">;
use super::super::*;
impl From<LockRecursionException> for System::Exception {
 fn from(v:LockRecursionException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,LockRecursionException>(v)
}} 
impl LockRecursionException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ManualResetEvent =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ManualResetEvent">;
use super::super::*;
impl From<ManualResetEvent> for System::Threading::EventWaitHandle {
 fn from(v:ManualResetEvent)->System::Threading::EventWaitHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Threading::EventWaitHandle,ManualResetEvent>(v)
}} 
impl ManualResetEvent {
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ManualResetEventSlim =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ManualResetEventSlim">;
use super::super::*;
impl From<ManualResetEventSlim> for System::Object {
 fn from(v:ManualResetEventSlim)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ManualResetEventSlim>(v)
}} 
impl ManualResetEventSlim {
    pub fn get_wait_handle(self) -> System::Threading::WaitHandle { self.instance0::<"get_WaitHandle", System::Threading::WaitHandle>() }
    pub fn get_is_set(self) -> bool { self.instance0::<"get_IsSet", bool>() }
    pub fn get_spin_count(self) -> i32 { self.instance0::<"get_SpinCount", i32>() }
    pub fn set(self) { self.instance0::<"Set", ()>() }
    pub fn reset(self) { self.instance0::<"Reset", ()>() }
    pub fn wait(self) { self.instance0::<"Wait", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Mutex =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Mutex">;
use super::super::*;
impl From<Mutex> for System::Threading::WaitHandle {
 fn from(v:Mutex)->System::Threading::WaitHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Threading::WaitHandle,Mutex>(v)
}} 
impl Mutex {
    pub fn open_existing(a1: System::String) -> System::Threading::Mutex { Self::static1::<"OpenExisting", System::String, System::Threading::Mutex>(a1) }
    pub fn release_mutex(self) { self.instance0::<"ReleaseMutex", ()>() }
    pub fn new(a1: bool, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type Overlapped =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Overlapped">;
use super::super::*;
impl From<Overlapped> for System::Object {
 fn from(v:Overlapped)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Overlapped>(v)
}} 
impl Overlapped {
    pub fn get_async_result(self) -> System::IAsyncResult { self.instance0::<"get_AsyncResult", System::IAsyncResult>() }
    pub fn set_async_result(self, a1: System::IAsyncResult) { self.instance1::<"set_AsyncResult", System::IAsyncResult, ()>(a1) }
    pub fn get_offset_low(self) -> i32 { self.instance0::<"get_OffsetLow", i32>() }
    pub fn set_offset_low(self, a1: i32) { self.instance1::<"set_OffsetLow", i32, ()>(a1) }
    pub fn get_offset_high(self) -> i32 { self.instance0::<"get_OffsetHigh", i32>() }
    pub fn set_offset_high(self, a1: i32) { self.instance1::<"set_OffsetHigh", i32, ()>(a1) }
    pub fn get_event_handle(self) -> i32 { self.instance0::<"get_EventHandle", i32>() }
    pub fn set_event_handle(self, a1: i32) { self.instance1::<"set_EventHandle", i32, ()>(a1) }
    pub fn get_event_handle_int_ptr(self) -> isize { self.instance0::<"get_EventHandleIntPtr", isize>() }
    pub fn set_event_handle_int_ptr(self, a1: isize) { self.instance1::<"set_EventHandleIntPtr", isize, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ParameterizedThreadStart =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ParameterizedThreadStart">;
use super::super::*;
impl From<ParameterizedThreadStart> for System::MulticastDelegate {
 fn from(v:ParameterizedThreadStart)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ParameterizedThreadStart>(v)
}} 
impl ParameterizedThreadStart {
    pub fn invoke(self, a1: System::Object) { self.instance1::<"Invoke", System::Object, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ReaderWriterLockSlim =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ReaderWriterLockSlim">;
use super::super::*;
impl From<ReaderWriterLockSlim> for System::Object {
 fn from(v:ReaderWriterLockSlim)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ReaderWriterLockSlim>(v)
}} 
impl ReaderWriterLockSlim {
    pub fn enter_read_lock(self) { self.instance0::<"EnterReadLock", ()>() }
    pub fn try_enter_read_lock(self, a1: i32) -> bool { self.instance1::<"TryEnterReadLock", i32, bool>(a1) }
    pub fn enter_write_lock(self) { self.instance0::<"EnterWriteLock", ()>() }
    pub fn try_enter_write_lock(self, a1: i32) -> bool { self.instance1::<"TryEnterWriteLock", i32, bool>(a1) }
    pub fn enter_upgradeable_read_lock(self) { self.instance0::<"EnterUpgradeableReadLock", ()>() }
    pub fn try_enter_upgradeable_read_lock(self, a1: i32) -> bool { self.instance1::<"TryEnterUpgradeableReadLock", i32, bool>(a1) }
    pub fn exit_read_lock(self) { self.instance0::<"ExitReadLock", ()>() }
    pub fn exit_write_lock(self) { self.instance0::<"ExitWriteLock", ()>() }
    pub fn exit_upgradeable_read_lock(self) { self.instance0::<"ExitUpgradeableReadLock", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn get_is_read_lock_held(self) -> bool { self.instance0::<"get_IsReadLockHeld", bool>() }
    pub fn get_is_upgradeable_read_lock_held(self) -> bool { self.instance0::<"get_IsUpgradeableReadLockHeld", bool>() }
    pub fn get_is_write_lock_held(self) -> bool { self.instance0::<"get_IsWriteLockHeld", bool>() }
    pub fn get_current_read_count(self) -> i32 { self.instance0::<"get_CurrentReadCount", i32>() }
    pub fn get_recursive_read_count(self) -> i32 { self.instance0::<"get_RecursiveReadCount", i32>() }
    pub fn get_recursive_upgrade_count(self) -> i32 { self.instance0::<"get_RecursiveUpgradeCount", i32>() }
    pub fn get_recursive_write_count(self) -> i32 { self.instance0::<"get_RecursiveWriteCount", i32>() }
    pub fn get_waiting_read_count(self) -> i32 { self.instance0::<"get_WaitingReadCount", i32>() }
    pub fn get_waiting_upgrade_count(self) -> i32 { self.instance0::<"get_WaitingUpgradeCount", i32>() }
    pub fn get_waiting_write_count(self) -> i32 { self.instance0::<"get_WaitingWriteCount", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Semaphore =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Semaphore">;
use super::super::*;
impl From<Semaphore> for System::Threading::WaitHandle {
 fn from(v:Semaphore)->System::Threading::WaitHandle{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Threading::WaitHandle,Semaphore>(v)
}} 
impl Semaphore {
    pub fn open_existing(a1: System::String) -> System::Threading::Semaphore { Self::static1::<"OpenExisting", System::String, System::Threading::Semaphore>(a1) }
    pub fn release(self) -> i32 { self.instance0::<"Release", i32>() }
    pub fn new(a1: i32, a2: i32) -> Self { Self::ctor2(a1, a2) }
}
pub type SemaphoreFullException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.SemaphoreFullException">;
use super::super::*;
impl From<SemaphoreFullException> for System::SystemException {
 fn from(v:SemaphoreFullException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SemaphoreFullException>(v)
}} 
impl SemaphoreFullException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SemaphoreSlim =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.SemaphoreSlim">;
use super::super::*;
impl From<SemaphoreSlim> for System::Object {
 fn from(v:SemaphoreSlim)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SemaphoreSlim>(v)
}} 
impl SemaphoreSlim {
    pub fn get_current_count(self) -> i32 { self.instance0::<"get_CurrentCount", i32>() }
    pub fn get_available_wait_handle(self) -> System::Threading::WaitHandle { self.instance0::<"get_AvailableWaitHandle", System::Threading::WaitHandle>() }
    pub fn wait(self) { self.instance0::<"Wait", ()>() }
    pub fn wait_async(self) -> System::Threading::Tasks::Task { self.instance0::<"WaitAsync", System::Threading::Tasks::Task>() }
    pub fn release(self) -> i32 { self.instance0::<"Release", i32>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type SendOrPostCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.SendOrPostCallback">;
use super::super::*;
impl From<SendOrPostCallback> for System::MulticastDelegate {
 fn from(v:SendOrPostCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,SendOrPostCallback>(v)
}} 
impl SendOrPostCallback {
    pub fn invoke(self, a1: System::Object) { self.instance1::<"Invoke", System::Object, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type SynchronizationLockException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.SynchronizationLockException">;
use super::super::*;
impl From<SynchronizationLockException> for System::SystemException {
 fn from(v:SynchronizationLockException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SynchronizationLockException>(v)
}} 
impl SynchronizationLockException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ThreadAbortException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadAbortException">;
use super::super::*;
impl From<ThreadAbortException> for System::SystemException {
 fn from(v:ThreadAbortException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ThreadAbortException>(v)
}} 
impl ThreadAbortException {
    pub fn get_exception_state(self) -> System::Object { self.instance0::<"get_ExceptionState", System::Object>() }
}
pub type ThreadExceptionEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadExceptionEventArgs">;
use super::super::*;
impl From<ThreadExceptionEventArgs> for System::EventArgs {
 fn from(v:ThreadExceptionEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,ThreadExceptionEventArgs>(v)
}} 
impl ThreadExceptionEventArgs {
    pub fn get_exception(self) -> System::Exception { self.instance0::<"get_Exception", System::Exception>() }
    pub fn new(a1: System::Exception) -> Self { Self::ctor1(a1) }
}
pub type ThreadExceptionEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadExceptionEventHandler">;
use super::super::*;
impl From<ThreadExceptionEventHandler> for System::MulticastDelegate {
 fn from(v:ThreadExceptionEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ThreadExceptionEventHandler>(v)
}} 
impl ThreadExceptionEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::Threading::ThreadExceptionEventArgs) { self.instance2::<"Invoke", System::Object, System::Threading::ThreadExceptionEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ThreadInterruptedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadInterruptedException">;
use super::super::*;
impl From<ThreadInterruptedException> for System::SystemException {
 fn from(v:ThreadInterruptedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ThreadInterruptedException>(v)
}} 
impl ThreadInterruptedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type WaitCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.WaitCallback">;
use super::super::*;
impl From<WaitCallback> for System::MulticastDelegate {
 fn from(v:WaitCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,WaitCallback>(v)
}} 
impl WaitCallback {
    pub fn invoke(self, a1: System::Object) { self.instance1::<"Invoke", System::Object, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type WaitOrTimerCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.WaitOrTimerCallback">;
use super::super::*;
impl From<WaitOrTimerCallback> for System::MulticastDelegate {
 fn from(v:WaitOrTimerCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,WaitOrTimerCallback>(v)
}} 
impl WaitOrTimerCallback {
    pub fn invoke(self, a1: System::Object, a2: bool) { self.instance2::<"Invoke", System::Object, bool, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ThreadStart =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadStart">;
use super::super::*;
impl From<ThreadStart> for System::MulticastDelegate {
 fn from(v:ThreadStart)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ThreadStart>(v)
}} 
impl ThreadStart {
    pub fn invoke(self) { self.virt0::<"Invoke", ()>() }
    pub fn begin_invoke(self, a1: System::AsyncCallback, a2: System::Object) -> System::IAsyncResult { self.instance2::<"BeginInvoke", System::AsyncCallback, System::Object, System::IAsyncResult>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ThreadStartException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadStartException">;
use super::super::*;
impl From<ThreadStartException> for System::SystemException {
 fn from(v:ThreadStartException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ThreadStartException>(v)
}} 
pub type ThreadStateException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadStateException">;
use super::super::*;
impl From<ThreadStateException> for System::SystemException {
 fn from(v:ThreadStateException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ThreadStateException>(v)
}} 
impl ThreadStateException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Timeout =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Timeout">;
use super::super::*;
impl From<Timeout> for System::Object {
 fn from(v:Timeout)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Timeout>(v)
}} 
pub type PeriodicTimer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.PeriodicTimer">;
use super::super::*;
impl From<PeriodicTimer> for System::Object {
 fn from(v:PeriodicTimer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PeriodicTimer>(v)
}} 
impl PeriodicTimer {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
}
pub type TimerCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.TimerCallback">;
use super::super::*;
impl From<TimerCallback> for System::MulticastDelegate {
 fn from(v:TimerCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,TimerCallback>(v)
}} 
impl TimerCallback {
    pub fn invoke(self, a1: System::Object) { self.instance1::<"Invoke", System::Object, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type Timer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Timer">;
use super::super::*;
impl From<Timer> for System::MarshalByRefObject {
 fn from(v:Timer)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,Timer>(v)
}} 
impl Timer {
    pub fn change(self, a1: i32, a2: i32) -> bool { self.instance2::<"Change", i32, i32, bool>(a1, a2) }
    pub fn get_active_count() -> i64 { Self::static0::<"get_ActiveCount", i64>() }
    pub fn dispose(self, a1: System::Threading::WaitHandle) -> bool { self.instance1::<"Dispose", System::Threading::WaitHandle, bool>(a1) }
    pub fn new(a1: System::Threading::TimerCallback) -> Self { Self::ctor1(a1) }
}
pub type Volatile =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.Volatile">;
use super::super::*;
impl From<Volatile> for System::Object {
 fn from(v:Volatile)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Volatile>(v)
}} 
pub type WaitHandleCannotBeOpenedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.WaitHandleCannotBeOpenedException">;
use super::super::*;
impl From<WaitHandleCannotBeOpenedException> for System::ApplicationException {
 fn from(v:WaitHandleCannotBeOpenedException)->System::ApplicationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ApplicationException,WaitHandleCannotBeOpenedException>(v)
}} 
impl WaitHandleCannotBeOpenedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type WaitHandleExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.WaitHandleExtensions">;
use super::super::*;
impl From<WaitHandleExtensions> for System::Object {
 fn from(v:WaitHandleExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,WaitHandleExtensions>(v)
}} 
impl WaitHandleExtensions {
    pub fn get_safe_wait_handle(a1: System::Threading::WaitHandle) -> Microsoft::Win32::SafeHandles::SafeWaitHandle { Self::static1::<"GetSafeWaitHandle", System::Threading::WaitHandle, Microsoft::Win32::SafeHandles::SafeWaitHandle>(a1) }
    pub fn set_safe_wait_handle(a1: System::Threading::WaitHandle, a2: Microsoft::Win32::SafeHandles::SafeWaitHandle) { Self::static2::<"SetSafeWaitHandle", System::Threading::WaitHandle, Microsoft::Win32::SafeHandles::SafeWaitHandle, ()>(a1, a2) }
}
pub type ITimer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ITimer">;
use super::super::*;
pub type PreAllocatedOverlapped =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.PreAllocatedOverlapped">;
use super::super::*;
impl From<PreAllocatedOverlapped> for System::Object {
 fn from(v:PreAllocatedOverlapped)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PreAllocatedOverlapped>(v)
}} 
impl PreAllocatedOverlapped {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new(a1: System::Threading::IOCompletionCallback, a2: System::Object, a3: System::Object) -> Self { Self::ctor3(a1, a2, a3) }
}
pub type RegisteredWaitHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.RegisteredWaitHandle">;
use super::super::*;
impl From<RegisteredWaitHandle> for System::MarshalByRefObject {
 fn from(v:RegisteredWaitHandle)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,RegisteredWaitHandle>(v)
}} 
impl RegisteredWaitHandle {
    pub fn unregister(self, a1: System::Threading::WaitHandle) -> bool { self.instance1::<"Unregister", System::Threading::WaitHandle, bool>(a1) }
}
pub type ThreadPoolBoundHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Threading.ThreadPoolBoundHandle">;
use super::super::*;
impl From<ThreadPoolBoundHandle> for System::Object {
 fn from(v:ThreadPoolBoundHandle)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ThreadPoolBoundHandle>(v)
}} 
impl ThreadPoolBoundHandle {
    pub fn get_handle(self) -> System::Runtime::InteropServices::SafeHandle { self.instance0::<"get_Handle", System::Runtime::InteropServices::SafeHandle>() }
    pub fn bind_handle(a1: System::Runtime::InteropServices::SafeHandle) -> System::Threading::ThreadPoolBoundHandle { Self::static1::<"BindHandle", System::Runtime::InteropServices::SafeHandle, System::Threading::ThreadPoolBoundHandle>(a1) }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
}
pub type BarrierPostPhaseException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.BarrierPostPhaseException">;
use super::super::*;
impl From<BarrierPostPhaseException> for System::Exception {
 fn from(v:BarrierPostPhaseException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,BarrierPostPhaseException>(v)
}} 
impl BarrierPostPhaseException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Barrier =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.Barrier">;
use super::super::*;
impl From<Barrier> for System::Object {
 fn from(v:Barrier)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Barrier>(v)
}} 
impl Barrier {
    pub fn get_participants_remaining(self) -> i32 { self.instance0::<"get_ParticipantsRemaining", i32>() }
    pub fn get_participant_count(self) -> i32 { self.instance0::<"get_ParticipantCount", i32>() }
    pub fn get_current_phase_number(self) -> i64 { self.instance0::<"get_CurrentPhaseNumber", i64>() }
    pub fn add_participant(self) -> i64 { self.instance0::<"AddParticipant", i64>() }
    pub fn add_participants(self, a1: i32) -> i64 { self.instance1::<"AddParticipants", i32, i64>(a1) }
    pub fn remove_participant(self) { self.instance0::<"RemoveParticipant", ()>() }
    pub fn remove_participants(self, a1: i32) { self.instance1::<"RemoveParticipants", i32, ()>(a1) }
    pub fn signal_and_wait(self) { self.instance0::<"SignalAndWait", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type CountdownEvent =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.CountdownEvent">;
use super::super::*;
impl From<CountdownEvent> for System::Object {
 fn from(v:CountdownEvent)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CountdownEvent>(v)
}} 
impl CountdownEvent {
    pub fn get_current_count(self) -> i32 { self.instance0::<"get_CurrentCount", i32>() }
    pub fn get_initial_count(self) -> i32 { self.instance0::<"get_InitialCount", i32>() }
    pub fn get_is_set(self) -> bool { self.instance0::<"get_IsSet", bool>() }
    pub fn get_wait_handle(self) -> System::Threading::WaitHandle { self.instance0::<"get_WaitHandle", System::Threading::WaitHandle>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn signal(self) -> bool { self.instance0::<"Signal", bool>() }
    pub fn add_count(self) { self.instance0::<"AddCount", ()>() }
    pub fn try_add_count(self) -> bool { self.instance0::<"TryAddCount", bool>() }
    pub fn reset(self) { self.instance0::<"Reset", ()>() }
    pub fn wait(self) { self.instance0::<"Wait", ()>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type HostExecutionContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.HostExecutionContext">;
use super::super::*;
impl From<HostExecutionContext> for System::Object {
 fn from(v:HostExecutionContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,HostExecutionContext>(v)
}} 
impl HostExecutionContext {
    pub fn create_copy(self) -> System::Threading::HostExecutionContext { self.virt0::<"CreateCopy", System::Threading::HostExecutionContext>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type HostExecutionContextManager =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.HostExecutionContextManager">;
use super::super::*;
impl From<HostExecutionContextManager> for System::Object {
 fn from(v:HostExecutionContextManager)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,HostExecutionContextManager>(v)
}} 
impl HostExecutionContextManager {
    pub fn capture(self) -> System::Threading::HostExecutionContext { self.virt0::<"Capture", System::Threading::HostExecutionContext>() }
    pub fn set_host_execution_context(self, a1: System::Threading::HostExecutionContext) -> System::Object { self.instance1::<"SetHostExecutionContext", System::Threading::HostExecutionContext, System::Object>(a1) }
    pub fn revert(self, a1: System::Object) { self.instance1::<"Revert", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ReaderWriterLock =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Threading","System.Threading.ReaderWriterLock">;
use super::super::*;
impl From<ReaderWriterLock> for System::Runtime::ConstrainedExecution::CriticalFinalizerObject {
 fn from(v:ReaderWriterLock)->System::Runtime::ConstrainedExecution::CriticalFinalizerObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::ConstrainedExecution::CriticalFinalizerObject,ReaderWriterLock>(v)
}} 
impl ReaderWriterLock {
    pub fn get_is_reader_lock_held(self) -> bool { self.instance0::<"get_IsReaderLockHeld", bool>() }
    pub fn get_is_writer_lock_held(self) -> bool { self.instance0::<"get_IsWriterLockHeld", bool>() }
    pub fn get_writer_seq_num(self) -> i32 { self.instance0::<"get_WriterSeqNum", i32>() }
    pub fn any_writers_since(self, a1: i32) -> bool { self.instance1::<"AnyWritersSince", i32, bool>(a1) }
    pub fn acquire_reader_lock(self, a1: i32) { self.instance1::<"AcquireReaderLock", i32, ()>(a1) }
    pub fn acquire_writer_lock(self, a1: i32) { self.instance1::<"AcquireWriterLock", i32, ()>(a1) }
    pub fn release_reader_lock(self) { self.instance0::<"ReleaseReaderLock", ()>() }
    pub fn release_writer_lock(self) { self.instance0::<"ReleaseWriterLock", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod Text{
pub mod Unicode{
pub type Utf8 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.Unicode.Utf8">;
use super::super::super::*;
impl From<Utf8> for System::Object {
 fn from(v:Utf8)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Utf8>(v)
}} 
}
pub mod RegularExpressions{
pub type Capture =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.Capture">;
use super::super::super::*;
impl From<Capture> for System::Object {
 fn from(v:Capture)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Capture>(v)
}} 
impl Capture {
    pub fn get_index(self) -> i32 { self.instance0::<"get_Index", i32>() }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type CaptureCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.CaptureCollection">;
use super::super::super::*;
impl From<CaptureCollection> for System::Object {
 fn from(v:CaptureCollection)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CaptureCollection>(v)
}} 
impl CaptureCollection {
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_item(self, a1: i32) -> System::Text::RegularExpressions::Capture { self.instance1::<"get_Item", i32, System::Text::RegularExpressions::Capture>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
}
pub type Group =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.Group">;
use super::super::super::*;
impl From<Group> for System::Text::RegularExpressions::Capture {
 fn from(v:Group)->System::Text::RegularExpressions::Capture{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::RegularExpressions::Capture,Group>(v)
}} 
impl Group {
    pub fn get_success(self) -> bool { self.instance0::<"get_Success", bool>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_captures(self) -> System::Text::RegularExpressions::CaptureCollection { self.instance0::<"get_Captures", System::Text::RegularExpressions::CaptureCollection>() }
    pub fn synchronized(a1: System::Text::RegularExpressions::Group) -> System::Text::RegularExpressions::Group { Self::static1::<"Synchronized", System::Text::RegularExpressions::Group, System::Text::RegularExpressions::Group>(a1) }
}
pub type GroupCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.GroupCollection">;
use super::super::super::*;
impl From<GroupCollection> for System::Object {
 fn from(v:GroupCollection)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,GroupCollection>(v)
}} 
impl GroupCollection {
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_item(self, a1: i32) -> System::Text::RegularExpressions::Group { self.instance1::<"get_Item", i32, System::Text::RegularExpressions::Group>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn contains_key(self, a1: System::String) -> bool { self.instance1::<"ContainsKey", System::String, bool>(a1) }
}
pub type Match =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.Match">;
use super::super::super::*;
impl From<Match> for System::Text::RegularExpressions::Group {
 fn from(v:Match)->System::Text::RegularExpressions::Group{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::RegularExpressions::Group,Match>(v)
}} 
impl Match {
    pub fn get_empty() -> System::Text::RegularExpressions::Match { Self::static0::<"get_Empty", System::Text::RegularExpressions::Match>() }
    pub fn get_groups(self) -> System::Text::RegularExpressions::GroupCollection { self.virt0::<"get_Groups", System::Text::RegularExpressions::GroupCollection>() }
    pub fn next_match(self) -> System::Text::RegularExpressions::Match { self.instance0::<"NextMatch", System::Text::RegularExpressions::Match>() }
    pub fn result(self, a1: System::String) -> System::String { self.instance1::<"Result", System::String, System::String>(a1) }
    pub fn synchronized(a1: System::Text::RegularExpressions::Match) -> System::Text::RegularExpressions::Match { Self::static1::<"Synchronized", System::Text::RegularExpressions::Match, System::Text::RegularExpressions::Match>(a1) }
}
pub type MatchCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.MatchCollection">;
use super::super::super::*;
impl From<MatchCollection> for System::Object {
 fn from(v:MatchCollection)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MatchCollection>(v)
}} 
impl MatchCollection {
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_item(self, a1: i32) -> System::Text::RegularExpressions::Match { self.instance1::<"get_Item", i32, System::Text::RegularExpressions::Match>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
}
pub type Regex =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.Regex">;
use super::super::super::*;
impl From<Regex> for System::Object {
 fn from(v:Regex)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Regex>(v)
}} 
impl Regex {
    pub fn escape(a1: System::String) -> System::String { Self::static1::<"Escape", System::String, System::String>(a1) }
    pub fn unescape(a1: System::String) -> System::String { Self::static1::<"Unescape", System::String, System::String>(a1) }
    pub fn get_right_to_left(self) -> bool { self.instance0::<"get_RightToLeft", bool>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn group_name_from_number(self, a1: i32) -> System::String { self.instance1::<"GroupNameFromNumber", i32, System::String>(a1) }
    pub fn group_number_from_name(self, a1: System::String) -> i32 { self.instance1::<"GroupNumberFromName", System::String, i32>(a1) }
    pub fn get_cache_size() -> i32 { Self::static0::<"get_CacheSize", i32>() }
    pub fn set_cache_size(a1: i32) { Self::static1::<"set_CacheSize", i32, ()>(a1) }
    pub fn count(self, a1: System::String) -> i32 { self.instance1::<"Count", System::String, i32>(a1) }
    pub fn is_match(a1: System::String, a2: System::String) -> bool { Self::static2::<"IsMatch", System::String, System::String, bool>(a1, a2) }
    pub fn r#match(a1: System::String, a2: System::String) -> System::Text::RegularExpressions::Match { Self::static2::<"Match", System::String, System::String, System::Text::RegularExpressions::Match>(a1, a2) }
    pub fn matches(a1: System::String, a2: System::String) -> System::Text::RegularExpressions::MatchCollection { Self::static2::<"Matches", System::String, System::String, System::Text::RegularExpressions::MatchCollection>(a1, a2) }
    pub fn replace(self, a1: System::String, a2: System::String) -> System::String { self.instance2::<"Replace", System::String, System::String, System::String>(a1, a2) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type MatchEvaluator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.MatchEvaluator">;
use super::super::super::*;
impl From<MatchEvaluator> for System::MulticastDelegate {
 fn from(v:MatchEvaluator)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,MatchEvaluator>(v)
}} 
impl MatchEvaluator {
    pub fn invoke(self, a1: System::Text::RegularExpressions::Match) -> System::String { self.instance1::<"Invoke", System::Text::RegularExpressions::Match, System::String>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) -> System::String { self.instance1::<"EndInvoke", System::IAsyncResult, System::String>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type RegexCompilationInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.RegexCompilationInfo">;
use super::super::super::*;
impl From<RegexCompilationInfo> for System::Object {
 fn from(v:RegexCompilationInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RegexCompilationInfo>(v)
}} 
impl RegexCompilationInfo {
    pub fn get_is_public(self) -> bool { self.instance0::<"get_IsPublic", bool>() }
    pub fn set_is_public(self, a1: bool) { self.instance1::<"set_IsPublic", bool, ()>(a1) }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn get_namespace(self) -> System::String { self.instance0::<"get_Namespace", System::String>() }
    pub fn set_namespace(self, a1: System::String) { self.instance1::<"set_Namespace", System::String, ()>(a1) }
    pub fn get_pattern(self) -> System::String { self.instance0::<"get_Pattern", System::String>() }
    pub fn set_pattern(self, a1: System::String) { self.instance1::<"set_Pattern", System::String, ()>(a1) }
}
pub type GeneratedRegexAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.GeneratedRegexAttribute">;
use super::super::super::*;
impl From<GeneratedRegexAttribute> for System::Attribute {
 fn from(v:GeneratedRegexAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,GeneratedRegexAttribute>(v)
}} 
impl GeneratedRegexAttribute {
    pub fn get_pattern(self) -> System::String { self.instance0::<"get_Pattern", System::String>() }
    pub fn get_match_timeout_milliseconds(self) -> i32 { self.instance0::<"get_MatchTimeoutMilliseconds", i32>() }
    pub fn get_culture_name(self) -> System::String { self.instance0::<"get_CultureName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type RegexMatchTimeoutException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.RegexMatchTimeoutException">;
use super::super::super::*;
impl From<RegexMatchTimeoutException> for System::TimeoutException {
 fn from(v:RegexMatchTimeoutException)->System::TimeoutException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::TimeoutException,RegexMatchTimeoutException>(v)
}} 
impl RegexMatchTimeoutException {
    pub fn get_input(self) -> System::String { self.instance0::<"get_Input", System::String>() }
    pub fn get_pattern(self) -> System::String { self.instance0::<"get_Pattern", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type RegexParseException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.RegexParseException">;
use super::super::super::*;
impl From<RegexParseException> for System::ArgumentException {
 fn from(v:RegexParseException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,RegexParseException>(v)
}} 
impl RegexParseException {
    pub fn get_offset(self) -> i32 { self.instance0::<"get_Offset", i32>() }
}
pub type RegexRunner =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.RegexRunner">;
use super::super::super::*;
impl From<RegexRunner> for System::Object {
 fn from(v:RegexRunner)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RegexRunner>(v)
}} 
pub type RegexRunnerFactory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Text.RegularExpressions","System.Text.RegularExpressions.RegexRunnerFactory">;
use super::super::super::*;
impl From<RegexRunnerFactory> for System::Object {
 fn from(v:RegexRunnerFactory)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RegexRunnerFactory>(v)
}} 
}
pub type StringBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.StringBuilder">;
use super::super::*;
impl From<StringBuilder> for System::Object {
 fn from(v:StringBuilder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringBuilder>(v)
}} 
impl StringBuilder {
    pub fn get_capacity(self) -> i32 { self.instance0::<"get_Capacity", i32>() }
    pub fn set_capacity(self, a1: i32) { self.instance1::<"set_Capacity", i32, ()>(a1) }
    pub fn get_max_capacity(self) -> i32 { self.instance0::<"get_MaxCapacity", i32>() }
    pub fn ensure_capacity(self, a1: i32) -> i32 { self.instance1::<"EnsureCapacity", i32, i32>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn clear(self) -> System::Text::StringBuilder { self.instance0::<"Clear", System::Text::StringBuilder>() }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn set_length(self, a1: i32) { self.instance1::<"set_Length", i32, ()>(a1) }
    pub fn append(self, a1: System::String) -> System::Text::StringBuilder { self.instance1::<"Append", System::String, System::Text::StringBuilder>(a1) }
    pub fn append_line(self) -> System::Text::StringBuilder { self.instance0::<"AppendLine", System::Text::StringBuilder>() }
    pub fn remove(self, a1: i32, a2: i32) -> System::Text::StringBuilder { self.instance2::<"Remove", i32, i32, System::Text::StringBuilder>(a1, a2) }
    pub fn insert(self, a1: i32, a2: System::String) -> System::Text::StringBuilder { self.instance2::<"Insert", i32, System::String, System::Text::StringBuilder>(a1, a2) }
    pub fn append_format(self, a1: System::String, a2: System::Object) -> System::Text::StringBuilder { self.instance2::<"AppendFormat", System::String, System::Object, System::Text::StringBuilder>(a1, a2) }
    pub fn replace(self, a1: System::String, a2: System::String) -> System::Text::StringBuilder { self.instance2::<"Replace", System::String, System::String, System::Text::StringBuilder>(a1, a2) }
    pub fn equals(self, a1: System::Text::StringBuilder) -> bool { self.instance1::<"Equals", System::Text::StringBuilder, bool>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Ascii =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.Ascii">;
use super::super::*;
impl From<Ascii> for System::Object {
 fn from(v:Ascii)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Ascii>(v)
}} 
impl Ascii {
    pub fn is_valid(a1: u8) -> bool { Self::static1::<"IsValid", u8, bool>(a1) }
}
pub type ASCIIEncoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.ASCIIEncoding">;
use super::super::*;
impl From<ASCIIEncoding> for System::Text::Encoding {
 fn from(v:ASCIIEncoding)->System::Text::Encoding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::Encoding,ASCIIEncoding>(v)
}} 
impl ASCIIEncoding {
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_max_byte_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxByteCount", i32, i32>(a1) }
    pub fn get_max_char_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxCharCount", i32, i32>(a1) }
    pub fn get_is_single_byte(self) -> bool { self.virt0::<"get_IsSingleByte", bool>() }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type CompositeFormat =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.CompositeFormat">;
use super::super::*;
impl From<CompositeFormat> for System::Object {
 fn from(v:CompositeFormat)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CompositeFormat>(v)
}} 
impl CompositeFormat {
    pub fn parse(a1: System::String) -> System::Text::CompositeFormat { Self::static1::<"Parse", System::String, System::Text::CompositeFormat>(a1) }
    pub fn get_format(self) -> System::String { self.instance0::<"get_Format", System::String>() }
    pub fn get_minimum_argument_count(self) -> i32 { self.instance0::<"get_MinimumArgumentCount", i32>() }
}
pub type Decoder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.Decoder">;
use super::super::*;
impl From<Decoder> for System::Object {
 fn from(v:Decoder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Decoder>(v)
}} 
impl Decoder {
    pub fn get_fallback(self) -> System::Text::DecoderFallback { self.instance0::<"get_Fallback", System::Text::DecoderFallback>() }
    pub fn set_fallback(self, a1: System::Text::DecoderFallback) { self.instance1::<"set_Fallback", System::Text::DecoderFallback, ()>(a1) }
    pub fn get_fallback_buffer(self) -> System::Text::DecoderFallbackBuffer { self.instance0::<"get_FallbackBuffer", System::Text::DecoderFallbackBuffer>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type DecoderExceptionFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderExceptionFallback">;
use super::super::*;
impl From<DecoderExceptionFallback> for System::Text::DecoderFallback {
 fn from(v:DecoderExceptionFallback)->System::Text::DecoderFallback{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::DecoderFallback,DecoderExceptionFallback>(v)
}} 
impl DecoderExceptionFallback {
    pub fn create_fallback_buffer(self) -> System::Text::DecoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::DecoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DecoderExceptionFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderExceptionFallbackBuffer">;
use super::super::*;
impl From<DecoderExceptionFallbackBuffer> for System::Text::DecoderFallbackBuffer {
 fn from(v:DecoderExceptionFallbackBuffer)->System::Text::DecoderFallbackBuffer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::DecoderFallbackBuffer,DecoderExceptionFallbackBuffer>(v)
}} 
impl DecoderExceptionFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DecoderFallbackException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderFallbackException">;
use super::super::*;
impl From<DecoderFallbackException> for System::ArgumentException {
 fn from(v:DecoderFallbackException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,DecoderFallbackException>(v)
}} 
impl DecoderFallbackException {
    pub fn get_index(self) -> i32 { self.instance0::<"get_Index", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DecoderFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderFallback">;
use super::super::*;
impl From<DecoderFallback> for System::Object {
 fn from(v:DecoderFallback)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DecoderFallback>(v)
}} 
impl DecoderFallback {
    pub fn get_replacement_fallback() -> System::Text::DecoderFallback { Self::static0::<"get_ReplacementFallback", System::Text::DecoderFallback>() }
    pub fn get_exception_fallback() -> System::Text::DecoderFallback { Self::static0::<"get_ExceptionFallback", System::Text::DecoderFallback>() }
    pub fn create_fallback_buffer(self) -> System::Text::DecoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::DecoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
}
pub type DecoderFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderFallbackBuffer">;
use super::super::*;
impl From<DecoderFallbackBuffer> for System::Object {
 fn from(v:DecoderFallbackBuffer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DecoderFallbackBuffer>(v)
}} 
impl DecoderFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type DecoderReplacementFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderReplacementFallback">;
use super::super::*;
impl From<DecoderReplacementFallback> for System::Text::DecoderFallback {
 fn from(v:DecoderReplacementFallback)->System::Text::DecoderFallback{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::DecoderFallback,DecoderReplacementFallback>(v)
}} 
impl DecoderReplacementFallback {
    pub fn get_default_string(self) -> System::String { self.instance0::<"get_DefaultString", System::String>() }
    pub fn create_fallback_buffer(self) -> System::Text::DecoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::DecoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DecoderReplacementFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.DecoderReplacementFallbackBuffer">;
use super::super::*;
impl From<DecoderReplacementFallbackBuffer> for System::Text::DecoderFallbackBuffer {
 fn from(v:DecoderReplacementFallbackBuffer)->System::Text::DecoderFallbackBuffer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::DecoderFallbackBuffer,DecoderReplacementFallbackBuffer>(v)
}} 
impl DecoderReplacementFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
    pub fn new(a1: System::Text::DecoderReplacementFallback) -> Self { Self::ctor1(a1) }
}
pub type Encoder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.Encoder">;
use super::super::*;
impl From<Encoder> for System::Object {
 fn from(v:Encoder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Encoder>(v)
}} 
impl Encoder {
    pub fn get_fallback(self) -> System::Text::EncoderFallback { self.instance0::<"get_Fallback", System::Text::EncoderFallback>() }
    pub fn set_fallback(self, a1: System::Text::EncoderFallback) { self.instance1::<"set_Fallback", System::Text::EncoderFallback, ()>(a1) }
    pub fn get_fallback_buffer(self) -> System::Text::EncoderFallbackBuffer { self.instance0::<"get_FallbackBuffer", System::Text::EncoderFallbackBuffer>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type EncoderExceptionFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderExceptionFallback">;
use super::super::*;
impl From<EncoderExceptionFallback> for System::Text::EncoderFallback {
 fn from(v:EncoderExceptionFallback)->System::Text::EncoderFallback{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::EncoderFallback,EncoderExceptionFallback>(v)
}} 
impl EncoderExceptionFallback {
    pub fn create_fallback_buffer(self) -> System::Text::EncoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::EncoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EncoderExceptionFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderExceptionFallbackBuffer">;
use super::super::*;
impl From<EncoderExceptionFallbackBuffer> for System::Text::EncoderFallbackBuffer {
 fn from(v:EncoderExceptionFallbackBuffer)->System::Text::EncoderFallbackBuffer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::EncoderFallbackBuffer,EncoderExceptionFallbackBuffer>(v)
}} 
impl EncoderExceptionFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EncoderFallbackException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderFallbackException">;
use super::super::*;
impl From<EncoderFallbackException> for System::ArgumentException {
 fn from(v:EncoderFallbackException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,EncoderFallbackException>(v)
}} 
impl EncoderFallbackException {
    pub fn get_index(self) -> i32 { self.instance0::<"get_Index", i32>() }
    pub fn is_unknown_surrogate(self) -> bool { self.instance0::<"IsUnknownSurrogate", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EncoderFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderFallback">;
use super::super::*;
impl From<EncoderFallback> for System::Object {
 fn from(v:EncoderFallback)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EncoderFallback>(v)
}} 
impl EncoderFallback {
    pub fn get_replacement_fallback() -> System::Text::EncoderFallback { Self::static0::<"get_ReplacementFallback", System::Text::EncoderFallback>() }
    pub fn get_exception_fallback() -> System::Text::EncoderFallback { Self::static0::<"get_ExceptionFallback", System::Text::EncoderFallback>() }
    pub fn create_fallback_buffer(self) -> System::Text::EncoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::EncoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
}
pub type EncoderFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderFallbackBuffer">;
use super::super::*;
impl From<EncoderFallbackBuffer> for System::Object {
 fn from(v:EncoderFallbackBuffer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EncoderFallbackBuffer>(v)
}} 
impl EncoderFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type EncoderReplacementFallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderReplacementFallback">;
use super::super::*;
impl From<EncoderReplacementFallback> for System::Text::EncoderFallback {
 fn from(v:EncoderReplacementFallback)->System::Text::EncoderFallback{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::EncoderFallback,EncoderReplacementFallback>(v)
}} 
impl EncoderReplacementFallback {
    pub fn get_default_string(self) -> System::String { self.instance0::<"get_DefaultString", System::String>() }
    pub fn create_fallback_buffer(self) -> System::Text::EncoderFallbackBuffer { self.virt0::<"CreateFallbackBuffer", System::Text::EncoderFallbackBuffer>() }
    pub fn get_max_char_count(self) -> i32 { self.virt0::<"get_MaxCharCount", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EncoderReplacementFallbackBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncoderReplacementFallbackBuffer">;
use super::super::*;
impl From<EncoderReplacementFallbackBuffer> for System::Text::EncoderFallbackBuffer {
 fn from(v:EncoderReplacementFallbackBuffer)->System::Text::EncoderFallbackBuffer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::EncoderFallbackBuffer,EncoderReplacementFallbackBuffer>(v)
}} 
impl EncoderReplacementFallbackBuffer {
    pub fn move_previous(self) -> bool { self.virt0::<"MovePrevious", bool>() }
    pub fn get_remaining(self) -> i32 { self.virt0::<"get_Remaining", i32>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
    pub fn new(a1: System::Text::EncoderReplacementFallback) -> Self { Self::ctor1(a1) }
}
pub type Encoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.Encoding">;
use super::super::*;
impl From<Encoding> for System::Object {
 fn from(v:Encoding)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Encoding>(v)
}} 
impl Encoding {
    pub fn get_default() -> System::Text::Encoding { Self::static0::<"get_Default", System::Text::Encoding>() }
    pub fn register_provider(a1: System::Text::EncodingProvider) { Self::static1::<"RegisterProvider", System::Text::EncodingProvider, ()>(a1) }
    pub fn get_encoding(a1: i32) -> System::Text::Encoding { Self::static1::<"GetEncoding", i32, System::Text::Encoding>(a1) }
    pub fn get_body_name(self) -> System::String { self.virt0::<"get_BodyName", System::String>() }
    pub fn get_encoding_name(self) -> System::String { self.virt0::<"get_EncodingName", System::String>() }
    pub fn get_header_name(self) -> System::String { self.virt0::<"get_HeaderName", System::String>() }
    pub fn get_web_name(self) -> System::String { self.virt0::<"get_WebName", System::String>() }
    pub fn get_windows_code_page(self) -> i32 { self.virt0::<"get_WindowsCodePage", i32>() }
    pub fn get_is_browser_display(self) -> bool { self.virt0::<"get_IsBrowserDisplay", bool>() }
    pub fn get_is_browser_save(self) -> bool { self.virt0::<"get_IsBrowserSave", bool>() }
    pub fn get_is_mail_news_display(self) -> bool { self.virt0::<"get_IsMailNewsDisplay", bool>() }
    pub fn get_is_mail_news_save(self) -> bool { self.virt0::<"get_IsMailNewsSave", bool>() }
    pub fn get_is_single_byte(self) -> bool { self.virt0::<"get_IsSingleByte", bool>() }
    pub fn get_encoder_fallback(self) -> System::Text::EncoderFallback { self.instance0::<"get_EncoderFallback", System::Text::EncoderFallback>() }
    pub fn set_encoder_fallback(self, a1: System::Text::EncoderFallback) { self.instance1::<"set_EncoderFallback", System::Text::EncoderFallback, ()>(a1) }
    pub fn get_decoder_fallback(self) -> System::Text::DecoderFallback { self.instance0::<"get_DecoderFallback", System::Text::DecoderFallback>() }
    pub fn set_decoder_fallback(self, a1: System::Text::DecoderFallback) { self.instance1::<"set_DecoderFallback", System::Text::DecoderFallback, ()>(a1) }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn get_ascii() -> System::Text::Encoding { Self::static0::<"get_ASCII", System::Text::Encoding>() }
    pub fn get_latin1() -> System::Text::Encoding { Self::static0::<"get_Latin1", System::Text::Encoding>() }
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_code_page(self) -> i32 { self.virt0::<"get_CodePage", i32>() }
    pub fn is_always_normalized(self) -> bool { self.instance0::<"IsAlwaysNormalized", bool>() }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn get_unicode() -> System::Text::Encoding { Self::static0::<"get_Unicode", System::Text::Encoding>() }
    pub fn get_big_endian_unicode() -> System::Text::Encoding { Self::static0::<"get_BigEndianUnicode", System::Text::Encoding>() }
    pub fn get_utf7() -> System::Text::Encoding { Self::static0::<"get_UTF7", System::Text::Encoding>() }
    pub fn get_utf8() -> System::Text::Encoding { Self::static0::<"get_UTF8", System::Text::Encoding>() }
    pub fn get_utf32() -> System::Text::Encoding { Self::static0::<"get_UTF32", System::Text::Encoding>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
}
pub type EncodingInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncodingInfo">;
use super::super::*;
impl From<EncodingInfo> for System::Object {
 fn from(v:EncodingInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EncodingInfo>(v)
}} 
impl EncodingInfo {
    pub fn get_code_page(self) -> i32 { self.instance0::<"get_CodePage", i32>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_display_name(self) -> System::String { self.instance0::<"get_DisplayName", System::String>() }
    pub fn get_encoding(self) -> System::Text::Encoding { self.instance0::<"GetEncoding", System::Text::Encoding>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
}
pub type EncodingProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.EncodingProvider">;
use super::super::*;
impl From<EncodingProvider> for System::Object {
 fn from(v:EncodingProvider)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EncodingProvider>(v)
}} 
impl EncodingProvider {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnicodeEncoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.UnicodeEncoding">;
use super::super::*;
impl From<UnicodeEncoding> for System::Text::Encoding {
 fn from(v:UnicodeEncoding)->System::Text::Encoding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::Encoding,UnicodeEncoding>(v)
}} 
impl UnicodeEncoding {
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_max_byte_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxByteCount", i32, i32>(a1) }
    pub fn get_max_char_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxCharCount", i32, i32>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UTF32Encoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.UTF32Encoding">;
use super::super::*;
impl From<UTF32Encoding> for System::Text::Encoding {
 fn from(v:UTF32Encoding)->System::Text::Encoding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::Encoding,UTF32Encoding>(v)
}} 
impl UTF32Encoding {
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn get_max_byte_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxByteCount", i32, i32>(a1) }
    pub fn get_max_char_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxCharCount", i32, i32>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UTF7Encoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.UTF7Encoding">;
use super::super::*;
impl From<UTF7Encoding> for System::Text::Encoding {
 fn from(v:UTF7Encoding)->System::Text::Encoding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::Encoding,UTF7Encoding>(v)
}} 
impl UTF7Encoding {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn get_max_byte_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxByteCount", i32, i32>(a1) }
    pub fn get_max_char_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxCharCount", i32, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UTF8Encoding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Text.UTF8Encoding">;
use super::super::*;
impl From<UTF8Encoding> for System::Text::Encoding {
 fn from(v:UTF8Encoding)->System::Text::Encoding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Text::Encoding,UTF8Encoding>(v)
}} 
impl UTF8Encoding {
    pub fn get_byte_count(self, a1: System::String) -> i32 { self.instance1::<"GetByteCount", System::String, i32>(a1) }
    pub fn get_decoder(self) -> System::Text::Decoder { self.virt0::<"GetDecoder", System::Text::Decoder>() }
    pub fn get_encoder(self) -> System::Text::Encoder { self.virt0::<"GetEncoder", System::Text::Encoder>() }
    pub fn get_max_byte_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxByteCount", i32, i32>(a1) }
    pub fn get_max_char_count(self, a1: i32) -> i32 { self.instance1::<"GetMaxCharCount", i32, i32>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EncodingExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Memory","System.Text.EncodingExtensions">;
use super::super::*;
impl From<EncodingExtensions> for System::Object {
 fn from(v:EncodingExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EncodingExtensions>(v)
}} 
}
pub mod Security{
pub mod Principal{
pub type IIdentity =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Principal.IIdentity">;
use super::super::super::*;
impl IIdentity {
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_authentication_type(self) -> System::String { self.virt0::<"get_AuthenticationType", System::String>() }
    pub fn get_is_authenticated(self) -> bool { self.virt0::<"get_IsAuthenticated", bool>() }
}
pub type IPrincipal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Principal.IPrincipal">;
use super::super::super::*;
impl IPrincipal {
    pub fn get_identity(self) -> System::Security::Principal::IIdentity { self.virt0::<"get_Identity", System::Security::Principal::IIdentity>() }
}
}
pub mod Permissions{
pub type CodeAccessSecurityAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Permissions.CodeAccessSecurityAttribute">;
use super::super::super::*;
impl From<CodeAccessSecurityAttribute> for System::Security::Permissions::SecurityAttribute {
 fn from(v:CodeAccessSecurityAttribute)->System::Security::Permissions::SecurityAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Security::Permissions::SecurityAttribute,CodeAccessSecurityAttribute>(v)
}} 
pub type SecurityAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Permissions.SecurityAttribute">;
use super::super::super::*;
impl From<SecurityAttribute> for System::Attribute {
 fn from(v:SecurityAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecurityAttribute>(v)
}} 
impl SecurityAttribute {
    pub fn get_unrestricted(self) -> bool { self.instance0::<"get_Unrestricted", bool>() }
    pub fn set_unrestricted(self, a1: bool) { self.instance1::<"set_Unrestricted", bool, ()>(a1) }
    pub fn create_permission(self) -> System::Security::IPermission { self.virt0::<"CreatePermission", System::Security::IPermission>() }
}
pub type SecurityPermissionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Permissions.SecurityPermissionAttribute">;
use super::super::super::*;
impl From<SecurityPermissionAttribute> for System::Security::Permissions::CodeAccessSecurityAttribute {
 fn from(v:SecurityPermissionAttribute)->System::Security::Permissions::CodeAccessSecurityAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Security::Permissions::CodeAccessSecurityAttribute,SecurityPermissionAttribute>(v)
}} 
impl SecurityPermissionAttribute {
    pub fn get_assertion(self) -> bool { self.instance0::<"get_Assertion", bool>() }
    pub fn set_assertion(self, a1: bool) { self.instance1::<"set_Assertion", bool, ()>(a1) }
    pub fn get_binding_redirects(self) -> bool { self.instance0::<"get_BindingRedirects", bool>() }
    pub fn set_binding_redirects(self, a1: bool) { self.instance1::<"set_BindingRedirects", bool, ()>(a1) }
    pub fn get_control_app_domain(self) -> bool { self.instance0::<"get_ControlAppDomain", bool>() }
    pub fn set_control_app_domain(self, a1: bool) { self.instance1::<"set_ControlAppDomain", bool, ()>(a1) }
    pub fn get_control_domain_policy(self) -> bool { self.instance0::<"get_ControlDomainPolicy", bool>() }
    pub fn set_control_domain_policy(self, a1: bool) { self.instance1::<"set_ControlDomainPolicy", bool, ()>(a1) }
    pub fn get_control_evidence(self) -> bool { self.instance0::<"get_ControlEvidence", bool>() }
    pub fn set_control_evidence(self, a1: bool) { self.instance1::<"set_ControlEvidence", bool, ()>(a1) }
    pub fn get_control_policy(self) -> bool { self.instance0::<"get_ControlPolicy", bool>() }
    pub fn set_control_policy(self, a1: bool) { self.instance1::<"set_ControlPolicy", bool, ()>(a1) }
    pub fn get_control_principal(self) -> bool { self.instance0::<"get_ControlPrincipal", bool>() }
    pub fn set_control_principal(self, a1: bool) { self.instance1::<"set_ControlPrincipal", bool, ()>(a1) }
    pub fn get_control_thread(self) -> bool { self.instance0::<"get_ControlThread", bool>() }
    pub fn set_control_thread(self, a1: bool) { self.instance1::<"set_ControlThread", bool, ()>(a1) }
    pub fn get_execution(self) -> bool { self.instance0::<"get_Execution", bool>() }
    pub fn set_execution(self, a1: bool) { self.instance1::<"set_Execution", bool, ()>(a1) }
    pub fn get_infrastructure(self) -> bool { self.instance0::<"get_Infrastructure", bool>() }
    pub fn set_infrastructure(self, a1: bool) { self.instance1::<"set_Infrastructure", bool, ()>(a1) }
    pub fn get_remoting_configuration(self) -> bool { self.instance0::<"get_RemotingConfiguration", bool>() }
    pub fn set_remoting_configuration(self, a1: bool) { self.instance1::<"set_RemotingConfiguration", bool, ()>(a1) }
    pub fn get_serialization_formatter(self) -> bool { self.instance0::<"get_SerializationFormatter", bool>() }
    pub fn set_serialization_formatter(self, a1: bool) { self.instance1::<"set_SerializationFormatter", bool, ()>(a1) }
    pub fn get_skip_verification(self) -> bool { self.instance0::<"get_SkipVerification", bool>() }
    pub fn set_skip_verification(self, a1: bool) { self.instance1::<"set_SkipVerification", bool, ()>(a1) }
    pub fn get_unmanaged_code(self) -> bool { self.instance0::<"get_UnmanagedCode", bool>() }
    pub fn set_unmanaged_code(self, a1: bool) { self.instance1::<"set_UnmanagedCode", bool, ()>(a1) }
    pub fn create_permission(self) -> System::Security::IPermission { self.virt0::<"CreatePermission", System::Security::IPermission>() }
}
}
pub mod Cryptography{
pub type CryptographicException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.Cryptography.CryptographicException">;
use super::super::super::*;
impl From<CryptographicException> for System::SystemException {
 fn from(v:CryptographicException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,CryptographicException>(v)
}} 
impl CryptographicException {
    pub fn new() -> Self { Self::ctor0() }
}
}
pub type AllowPartiallyTrustedCallersAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.AllowPartiallyTrustedCallersAttribute">;
use super::super::*;
impl From<AllowPartiallyTrustedCallersAttribute> for System::Attribute {
 fn from(v:AllowPartiallyTrustedCallersAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AllowPartiallyTrustedCallersAttribute>(v)
}} 
impl AllowPartiallyTrustedCallersAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IPermission =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.IPermission">;
use super::super::*;
impl IPermission {
    pub fn copy(self) -> System::Security::IPermission { self.virt0::<"Copy", System::Security::IPermission>() }
    pub fn demand(self) { self.virt0::<"Demand", ()>() }
}
pub type ISecurityEncodable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.ISecurityEncodable">;
use super::super::*;
impl ISecurityEncodable {
    pub fn to_xml(self) -> System::Security::SecurityElement { self.virt0::<"ToXml", System::Security::SecurityElement>() }
}
pub type IStackWalk =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.IStackWalk">;
use super::super::*;
impl IStackWalk {
    pub fn assert(self) { self.virt0::<"Assert", ()>() }
    pub fn demand(self) { self.virt0::<"Demand", ()>() }
    pub fn deny(self) { self.virt0::<"Deny", ()>() }
    pub fn permit_only(self) { self.virt0::<"PermitOnly", ()>() }
}
pub type PermissionSet =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.PermissionSet">;
use super::super::*;
impl From<PermissionSet> for System::Object {
 fn from(v:PermissionSet)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PermissionSet>(v)
}} 
impl PermissionSet {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn add_permission(self, a1: System::Security::IPermission) -> System::Security::IPermission { self.instance1::<"AddPermission", System::Security::IPermission, System::Security::IPermission>(a1) }
    pub fn assert(self) { self.virt0::<"Assert", ()>() }
    pub fn contains_non_code_access_permissions(self) -> bool { self.instance0::<"ContainsNonCodeAccessPermissions", bool>() }
    pub fn copy(self) -> System::Security::PermissionSet { self.virt0::<"Copy", System::Security::PermissionSet>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn demand(self) { self.virt0::<"Demand", ()>() }
    pub fn deny(self) { self.virt0::<"Deny", ()>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn from_xml(self, a1: System::Security::SecurityElement) { self.instance1::<"FromXml", System::Security::SecurityElement, ()>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn get_permission(self, a1: System::Type) -> System::Security::IPermission { self.instance1::<"GetPermission", System::Type, System::Security::IPermission>(a1) }
    pub fn intersect(self, a1: System::Security::PermissionSet) -> System::Security::PermissionSet { self.instance1::<"Intersect", System::Security::PermissionSet, System::Security::PermissionSet>(a1) }
    pub fn is_empty(self) -> bool { self.instance0::<"IsEmpty", bool>() }
    pub fn is_subset_of(self, a1: System::Security::PermissionSet) -> bool { self.instance1::<"IsSubsetOf", System::Security::PermissionSet, bool>(a1) }
    pub fn is_unrestricted(self) -> bool { self.instance0::<"IsUnrestricted", bool>() }
    pub fn permit_only(self) { self.virt0::<"PermitOnly", ()>() }
    pub fn remove_permission(self, a1: System::Type) -> System::Security::IPermission { self.instance1::<"RemovePermission", System::Type, System::Security::IPermission>(a1) }
    pub fn revert_assert() { Self::static0::<"RevertAssert", ()>() }
    pub fn set_permission(self, a1: System::Security::IPermission) -> System::Security::IPermission { self.instance1::<"SetPermission", System::Security::IPermission, System::Security::IPermission>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn to_xml(self) -> System::Security::SecurityElement { self.virt0::<"ToXml", System::Security::SecurityElement>() }
    pub fn union(self, a1: System::Security::PermissionSet) -> System::Security::PermissionSet { self.instance1::<"Union", System::Security::PermissionSet, System::Security::PermissionSet>(a1) }
    pub fn new(a1: System::Security::PermissionSet) -> Self { Self::ctor1(a1) }
}
pub type SecureString =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecureString">;
use super::super::*;
impl From<SecureString> for System::Object {
 fn from(v:SecureString)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SecureString>(v)
}} 
impl SecureString {
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn clear(self) { self.instance0::<"Clear", ()>() }
    pub fn copy(self) -> System::Security::SecureString { self.instance0::<"Copy", System::Security::SecureString>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn is_read_only(self) -> bool { self.instance0::<"IsReadOnly", bool>() }
    pub fn make_read_only(self) { self.instance0::<"MakeReadOnly", ()>() }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecurityCriticalAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityCriticalAttribute">;
use super::super::*;
impl From<SecurityCriticalAttribute> for System::Attribute {
 fn from(v:SecurityCriticalAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecurityCriticalAttribute>(v)
}} 
impl SecurityCriticalAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecurityElement =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityElement">;
use super::super::*;
impl From<SecurityElement> for System::Object {
 fn from(v:SecurityElement)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SecurityElement>(v)
}} 
impl SecurityElement {
    pub fn get_tag(self) -> System::String { self.instance0::<"get_Tag", System::String>() }
    pub fn set_tag(self, a1: System::String) { self.instance1::<"set_Tag", System::String, ()>(a1) }
    pub fn get_attributes(self) -> System::Collections::Hashtable { self.instance0::<"get_Attributes", System::Collections::Hashtable>() }
    pub fn set_attributes(self, a1: System::Collections::Hashtable) { self.instance1::<"set_Attributes", System::Collections::Hashtable, ()>(a1) }
    pub fn get_text(self) -> System::String { self.instance0::<"get_Text", System::String>() }
    pub fn set_text(self, a1: System::String) { self.instance1::<"set_Text", System::String, ()>(a1) }
    pub fn get_children(self) -> System::Collections::ArrayList { self.instance0::<"get_Children", System::Collections::ArrayList>() }
    pub fn set_children(self, a1: System::Collections::ArrayList) { self.instance1::<"set_Children", System::Collections::ArrayList, ()>(a1) }
    pub fn add_attribute(self, a1: System::String, a2: System::String) { self.instance2::<"AddAttribute", System::String, System::String, ()>(a1, a2) }
    pub fn add_child(self, a1: System::Security::SecurityElement) { self.instance1::<"AddChild", System::Security::SecurityElement, ()>(a1) }
    pub fn equal(self, a1: System::Security::SecurityElement) -> bool { self.instance1::<"Equal", System::Security::SecurityElement, bool>(a1) }
    pub fn copy(self) -> System::Security::SecurityElement { self.instance0::<"Copy", System::Security::SecurityElement>() }
    pub fn is_valid_tag(a1: System::String) -> bool { Self::static1::<"IsValidTag", System::String, bool>(a1) }
    pub fn is_valid_text(a1: System::String) -> bool { Self::static1::<"IsValidText", System::String, bool>(a1) }
    pub fn is_valid_attribute_name(a1: System::String) -> bool { Self::static1::<"IsValidAttributeName", System::String, bool>(a1) }
    pub fn is_valid_attribute_value(a1: System::String) -> bool { Self::static1::<"IsValidAttributeValue", System::String, bool>(a1) }
    pub fn escape(a1: System::String) -> System::String { Self::static1::<"Escape", System::String, System::String>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn attribute(self, a1: System::String) -> System::String { self.instance1::<"Attribute", System::String, System::String>(a1) }
    pub fn search_for_child_by_tag(self, a1: System::String) -> System::Security::SecurityElement { self.instance1::<"SearchForChildByTag", System::String, System::Security::SecurityElement>(a1) }
    pub fn search_for_text_of_tag(self, a1: System::String) -> System::String { self.instance1::<"SearchForTextOfTag", System::String, System::String>(a1) }
    pub fn from_string(a1: System::String) -> System::Security::SecurityElement { Self::static1::<"FromString", System::String, System::Security::SecurityElement>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SecurityException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityException">;
use super::super::*;
impl From<SecurityException> for System::SystemException {
 fn from(v:SecurityException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SecurityException>(v)
}} 
impl SecurityException {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_demanded(self) -> System::Object { self.instance0::<"get_Demanded", System::Object>() }
    pub fn set_demanded(self, a1: System::Object) { self.instance1::<"set_Demanded", System::Object, ()>(a1) }
    pub fn get_deny_set_instance(self) -> System::Object { self.instance0::<"get_DenySetInstance", System::Object>() }
    pub fn set_deny_set_instance(self, a1: System::Object) { self.instance1::<"set_DenySetInstance", System::Object, ()>(a1) }
    pub fn get_failed_assembly_info(self) -> System::Reflection::AssemblyName { self.instance0::<"get_FailedAssemblyInfo", System::Reflection::AssemblyName>() }
    pub fn set_failed_assembly_info(self, a1: System::Reflection::AssemblyName) { self.instance1::<"set_FailedAssemblyInfo", System::Reflection::AssemblyName, ()>(a1) }
    pub fn get_granted_set(self) -> System::String { self.instance0::<"get_GrantedSet", System::String>() }
    pub fn set_granted_set(self, a1: System::String) { self.instance1::<"set_GrantedSet", System::String, ()>(a1) }
    pub fn get_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Method", System::Reflection::MethodInfo>() }
    pub fn set_method(self, a1: System::Reflection::MethodInfo) { self.instance1::<"set_Method", System::Reflection::MethodInfo, ()>(a1) }
    pub fn get_permission_state(self) -> System::String { self.instance0::<"get_PermissionState", System::String>() }
    pub fn set_permission_state(self, a1: System::String) { self.instance1::<"set_PermissionState", System::String, ()>(a1) }
    pub fn get_permission_type(self) -> System::Type { self.instance0::<"get_PermissionType", System::Type>() }
    pub fn set_permission_type(self, a1: System::Type) { self.instance1::<"set_PermissionType", System::Type, ()>(a1) }
    pub fn get_permit_only_set_instance(self) -> System::Object { self.instance0::<"get_PermitOnlySetInstance", System::Object>() }
    pub fn set_permit_only_set_instance(self, a1: System::Object) { self.instance1::<"set_PermitOnlySetInstance", System::Object, ()>(a1) }
    pub fn get_refused_set(self) -> System::String { self.instance0::<"get_RefusedSet", System::String>() }
    pub fn set_refused_set(self, a1: System::String) { self.instance1::<"set_RefusedSet", System::String, ()>(a1) }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecurityRulesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityRulesAttribute">;
use super::super::*;
impl From<SecurityRulesAttribute> for System::Attribute {
 fn from(v:SecurityRulesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecurityRulesAttribute>(v)
}} 
impl SecurityRulesAttribute {
    pub fn get_skip_verification_in_full_trust(self) -> bool { self.instance0::<"get_SkipVerificationInFullTrust", bool>() }
    pub fn set_skip_verification_in_full_trust(self, a1: bool) { self.instance1::<"set_SkipVerificationInFullTrust", bool, ()>(a1) }
}
pub type SecuritySafeCriticalAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecuritySafeCriticalAttribute">;
use super::super::*;
impl From<SecuritySafeCriticalAttribute> for System::Attribute {
 fn from(v:SecuritySafeCriticalAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecuritySafeCriticalAttribute>(v)
}} 
impl SecuritySafeCriticalAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecurityTransparentAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityTransparentAttribute">;
use super::super::*;
impl From<SecurityTransparentAttribute> for System::Attribute {
 fn from(v:SecurityTransparentAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecurityTransparentAttribute>(v)
}} 
impl SecurityTransparentAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecurityTreatAsSafeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SecurityTreatAsSafeAttribute">;
use super::super::*;
impl From<SecurityTreatAsSafeAttribute> for System::Attribute {
 fn from(v:SecurityTreatAsSafeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SecurityTreatAsSafeAttribute>(v)
}} 
impl SecurityTreatAsSafeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SuppressUnmanagedCodeSecurityAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.SuppressUnmanagedCodeSecurityAttribute">;
use super::super::*;
impl From<SuppressUnmanagedCodeSecurityAttribute> for System::Attribute {
 fn from(v:SuppressUnmanagedCodeSecurityAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SuppressUnmanagedCodeSecurityAttribute>(v)
}} 
impl SuppressUnmanagedCodeSecurityAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnverifiableCodeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.UnverifiableCodeAttribute">;
use super::super::*;
impl From<UnverifiableCodeAttribute> for System::Attribute {
 fn from(v:UnverifiableCodeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnverifiableCodeAttribute>(v)
}} 
impl UnverifiableCodeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type VerificationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Security.VerificationException">;
use super::super::*;
impl From<VerificationException> for System::SystemException {
 fn from(v:VerificationException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,VerificationException>(v)
}} 
impl VerificationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SecureStringMarshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Security.SecureStringMarshal">;
use super::super::*;
impl From<SecureStringMarshal> for System::Object {
 fn from(v:SecureStringMarshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SecureStringMarshal>(v)
}} 
impl SecureStringMarshal {
    pub fn secure_string_to_co_task_mem_ansi(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToCoTaskMemAnsi", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_global_alloc_ansi(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToGlobalAllocAnsi", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_co_task_mem_unicode(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToCoTaskMemUnicode", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_global_alloc_unicode(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToGlobalAllocUnicode", System::Security::SecureString, isize>(a1) }
}
}
pub mod Runtime{
pub mod Serialization{
pub type IDeserializationCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.IDeserializationCallback">;
use super::super::super::*;
pub type IFormatterConverter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.IFormatterConverter">;
use super::super::super::*;
pub type IObjectReference =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.IObjectReference">;
use super::super::super::*;
pub type ISafeSerializationData =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.ISafeSerializationData">;
use super::super::super::*;
pub type ISerializable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.ISerializable">;
use super::super::super::*;
pub type OnDeserializedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.OnDeserializedAttribute">;
use super::super::super::*;
impl From<OnDeserializedAttribute> for System::Attribute {
 fn from(v:OnDeserializedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OnDeserializedAttribute>(v)
}} 
impl OnDeserializedAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OnDeserializingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.OnDeserializingAttribute">;
use super::super::super::*;
impl From<OnDeserializingAttribute> for System::Attribute {
 fn from(v:OnDeserializingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OnDeserializingAttribute>(v)
}} 
impl OnDeserializingAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OnSerializedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.OnSerializedAttribute">;
use super::super::super::*;
impl From<OnSerializedAttribute> for System::Attribute {
 fn from(v:OnSerializedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OnSerializedAttribute>(v)
}} 
impl OnSerializedAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OnSerializingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.OnSerializingAttribute">;
use super::super::super::*;
impl From<OnSerializingAttribute> for System::Attribute {
 fn from(v:OnSerializingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OnSerializingAttribute>(v)
}} 
impl OnSerializingAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OptionalFieldAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.OptionalFieldAttribute">;
use super::super::super::*;
impl From<OptionalFieldAttribute> for System::Attribute {
 fn from(v:OptionalFieldAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OptionalFieldAttribute>(v)
}} 
impl OptionalFieldAttribute {
    pub fn get_version_added(self) -> i32 { self.instance0::<"get_VersionAdded", i32>() }
    pub fn set_version_added(self, a1: i32) { self.instance1::<"set_VersionAdded", i32, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type SafeSerializationEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.SafeSerializationEventArgs">;
use super::super::super::*;
impl From<SafeSerializationEventArgs> for System::EventArgs {
 fn from(v:SafeSerializationEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,SafeSerializationEventArgs>(v)
}} 
impl SafeSerializationEventArgs {
    pub fn add_serialized_state(self, a1: System::Runtime::Serialization::ISafeSerializationData) { self.instance1::<"AddSerializedState", System::Runtime::Serialization::ISafeSerializationData, ()>(a1) }
}
pub type SerializationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.SerializationException">;
use super::super::super::*;
impl From<SerializationException> for System::SystemException {
 fn from(v:SerializationException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SerializationException>(v)
}} 
impl SerializationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SerializationInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.SerializationInfo">;
use super::super::super::*;
impl From<SerializationInfo> for System::Object {
 fn from(v:SerializationInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SerializationInfo>(v)
}} 
impl SerializationInfo {
    pub fn get_full_type_name(self) -> System::String { self.instance0::<"get_FullTypeName", System::String>() }
    pub fn set_full_type_name(self, a1: System::String) { self.instance1::<"set_FullTypeName", System::String, ()>(a1) }
    pub fn get_assembly_name(self) -> System::String { self.instance0::<"get_AssemblyName", System::String>() }
    pub fn set_assembly_name(self, a1: System::String) { self.instance1::<"set_AssemblyName", System::String, ()>(a1) }
    pub fn get_is_full_type_name_set_explicit(self) -> bool { self.instance0::<"get_IsFullTypeNameSetExplicit", bool>() }
    pub fn get_is_assembly_name_set_explicit(self) -> bool { self.instance0::<"get_IsAssemblyNameSetExplicit", bool>() }
    pub fn set_type(self, a1: System::Type) { self.instance1::<"SetType", System::Type, ()>(a1) }
    pub fn get_member_count(self) -> i32 { self.instance0::<"get_MemberCount", i32>() }
    pub fn get_object_type(self) -> System::Type { self.instance0::<"get_ObjectType", System::Type>() }
    pub fn get_enumerator(self) -> System::Runtime::Serialization::SerializationInfoEnumerator { self.instance0::<"GetEnumerator", System::Runtime::Serialization::SerializationInfoEnumerator>() }
    pub fn add_value(self, a1: System::String, a2: System::Object) { self.instance2::<"AddValue", System::String, System::Object, ()>(a1, a2) }
    pub fn get_value(self, a1: System::String, a2: System::Type) -> System::Object { self.instance2::<"GetValue", System::String, System::Type, System::Object>(a1, a2) }
    pub fn get_boolean(self, a1: System::String) -> bool { self.instance1::<"GetBoolean", System::String, bool>(a1) }
    pub fn get_sbyte(self, a1: System::String) -> i8 { self.instance1::<"GetSByte", System::String, i8>(a1) }
    pub fn get_byte(self, a1: System::String) -> u8 { self.instance1::<"GetByte", System::String, u8>(a1) }
    pub fn get_int16(self, a1: System::String) -> i16 { self.instance1::<"GetInt16", System::String, i16>(a1) }
    pub fn get_uint16(self, a1: System::String) -> u16 { self.instance1::<"GetUInt16", System::String, u16>(a1) }
    pub fn get_int32(self, a1: System::String) -> i32 { self.instance1::<"GetInt32", System::String, i32>(a1) }
    pub fn get_uint32(self, a1: System::String) -> u32 { self.instance1::<"GetUInt32", System::String, u32>(a1) }
    pub fn get_int64(self, a1: System::String) -> i64 { self.instance1::<"GetInt64", System::String, i64>(a1) }
    pub fn get_uint64(self, a1: System::String) -> u64 { self.instance1::<"GetUInt64", System::String, u64>(a1) }
    pub fn get_single(self, a1: System::String) -> f32 { self.instance1::<"GetSingle", System::String, f32>(a1) }
    pub fn get_double(self, a1: System::String) -> f64 { self.instance1::<"GetDouble", System::String, f64>(a1) }
    pub fn get_string(self, a1: System::String) -> System::String { self.instance1::<"GetString", System::String, System::String>(a1) }
    pub fn new(a1: System::Type, a2: System::Runtime::Serialization::IFormatterConverter) -> Self { Self::ctor2(a1, a2) }
}
pub type SerializationInfoEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Serialization.SerializationInfoEnumerator">;
use super::super::super::*;
impl From<SerializationInfoEnumerator> for System::Object {
 fn from(v:SerializationInfoEnumerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SerializationInfoEnumerator>(v)
}} 
impl SerializationInfoEnumerator {
    pub fn move_next(self) -> bool { self.virt0::<"MoveNext", bool>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_value(self) -> System::Object { self.instance0::<"get_Value", System::Object>() }
    pub fn get_object_type(self) -> System::Type { self.instance0::<"get_ObjectType", System::Type>() }
}
}
pub mod Remoting{
pub type ObjectHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Remoting.ObjectHandle">;
use super::super::super::*;
impl From<ObjectHandle> for System::MarshalByRefObject {
 fn from(v:ObjectHandle)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,ObjectHandle>(v)
}} 
impl ObjectHandle {
    pub fn unwrap(self) -> System::Object { self.instance0::<"Unwrap", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
}
pub mod ExceptionServices{
pub type ExceptionDispatchInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ExceptionServices.ExceptionDispatchInfo">;
use super::super::super::*;
impl From<ExceptionDispatchInfo> for System::Object {
 fn from(v:ExceptionDispatchInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExceptionDispatchInfo>(v)
}} 
impl ExceptionDispatchInfo {
    pub fn capture(a1: System::Exception) -> System::Runtime::ExceptionServices::ExceptionDispatchInfo { Self::static1::<"Capture", System::Exception, System::Runtime::ExceptionServices::ExceptionDispatchInfo>(a1) }
    pub fn get_source_exception(self) -> System::Exception { self.instance0::<"get_SourceException", System::Exception>() }
    pub fn throw(self) { self.instance0::<"Throw", ()>() }
    pub fn set_current_stack_trace(a1: System::Exception) -> System::Exception { Self::static1::<"SetCurrentStackTrace", System::Exception, System::Exception>(a1) }
    pub fn set_remote_stack_trace(a1: System::Exception, a2: System::String) -> System::Exception { Self::static2::<"SetRemoteStackTrace", System::Exception, System::String, System::Exception>(a1, a2) }
}
pub type FirstChanceExceptionEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ExceptionServices.FirstChanceExceptionEventArgs">;
use super::super::super::*;
impl From<FirstChanceExceptionEventArgs> for System::EventArgs {
 fn from(v:FirstChanceExceptionEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,FirstChanceExceptionEventArgs>(v)
}} 
impl FirstChanceExceptionEventArgs {
    pub fn get_exception(self) -> System::Exception { self.instance0::<"get_Exception", System::Exception>() }
    pub fn new(a1: System::Exception) -> Self { Self::ctor1(a1) }
}
pub type HandleProcessCorruptedStateExceptionsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ExceptionServices.HandleProcessCorruptedStateExceptionsAttribute">;
use super::super::super::*;
impl From<HandleProcessCorruptedStateExceptionsAttribute> for System::Attribute {
 fn from(v:HandleProcessCorruptedStateExceptionsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,HandleProcessCorruptedStateExceptionsAttribute>(v)
}} 
impl HandleProcessCorruptedStateExceptionsAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod ConstrainedExecution{
pub type CriticalFinalizerObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ConstrainedExecution.CriticalFinalizerObject">;
use super::super::super::*;
impl From<CriticalFinalizerObject> for System::Object {
 fn from(v:CriticalFinalizerObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CriticalFinalizerObject>(v)
}} 
pub type PrePrepareMethodAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ConstrainedExecution.PrePrepareMethodAttribute">;
use super::super::super::*;
impl From<PrePrepareMethodAttribute> for System::Attribute {
 fn from(v:PrePrepareMethodAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,PrePrepareMethodAttribute>(v)
}} 
impl PrePrepareMethodAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ReliabilityContractAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ConstrainedExecution.ReliabilityContractAttribute">;
use super::super::super::*;
impl From<ReliabilityContractAttribute> for System::Attribute {
 fn from(v:ReliabilityContractAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ReliabilityContractAttribute>(v)
}} 
}
pub mod Versioning{
pub type ComponentGuaranteesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.ComponentGuaranteesAttribute">;
use super::super::super::*;
impl From<ComponentGuaranteesAttribute> for System::Attribute {
 fn from(v:ComponentGuaranteesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComponentGuaranteesAttribute>(v)
}} 
pub type FrameworkName =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.FrameworkName">;
use super::super::super::*;
impl From<FrameworkName> for System::Object {
 fn from(v:FrameworkName)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,FrameworkName>(v)
}} 
impl FrameworkName {
    pub fn get_identifier(self) -> System::String { self.instance0::<"get_Identifier", System::String>() }
    pub fn get_version(self) -> System::Version { self.instance0::<"get_Version", System::Version>() }
    pub fn get_profile(self) -> System::String { self.instance0::<"get_Profile", System::String>() }
    pub fn get_full_name(self) -> System::String { self.instance0::<"get_FullName", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn op_equality(a1: System::Runtime::Versioning::FrameworkName, a2: System::Runtime::Versioning::FrameworkName) -> bool { Self::static2::<"op_Equality", System::Runtime::Versioning::FrameworkName, System::Runtime::Versioning::FrameworkName, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Runtime::Versioning::FrameworkName, a2: System::Runtime::Versioning::FrameworkName) -> bool { Self::static2::<"op_Inequality", System::Runtime::Versioning::FrameworkName, System::Runtime::Versioning::FrameworkName, bool>(a1, a2) }
    pub fn new(a1: System::String, a2: System::Version) -> Self { Self::ctor2(a1, a2) }
}
pub type OSPlatformAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.OSPlatformAttribute">;
use super::super::super::*;
impl From<OSPlatformAttribute> for System::Attribute {
 fn from(v:OSPlatformAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OSPlatformAttribute>(v)
}} 
impl OSPlatformAttribute {
    pub fn get_platform_name(self) -> System::String { self.instance0::<"get_PlatformName", System::String>() }
}
pub type TargetPlatformAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.TargetPlatformAttribute">;
use super::super::super::*;
impl From<TargetPlatformAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:TargetPlatformAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,TargetPlatformAttribute>(v)
}} 
impl TargetPlatformAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SupportedOSPlatformAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.SupportedOSPlatformAttribute">;
use super::super::super::*;
impl From<SupportedOSPlatformAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:SupportedOSPlatformAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,SupportedOSPlatformAttribute>(v)
}} 
impl SupportedOSPlatformAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type UnsupportedOSPlatformAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.UnsupportedOSPlatformAttribute">;
use super::super::super::*;
impl From<UnsupportedOSPlatformAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:UnsupportedOSPlatformAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,UnsupportedOSPlatformAttribute>(v)
}} 
impl UnsupportedOSPlatformAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ObsoletedOSPlatformAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.ObsoletedOSPlatformAttribute">;
use super::super::super::*;
impl From<ObsoletedOSPlatformAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:ObsoletedOSPlatformAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,ObsoletedOSPlatformAttribute>(v)
}} 
impl ObsoletedOSPlatformAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SupportedOSPlatformGuardAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.SupportedOSPlatformGuardAttribute">;
use super::super::super::*;
impl From<SupportedOSPlatformGuardAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:SupportedOSPlatformGuardAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,SupportedOSPlatformGuardAttribute>(v)
}} 
impl SupportedOSPlatformGuardAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type UnsupportedOSPlatformGuardAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.UnsupportedOSPlatformGuardAttribute">;
use super::super::super::*;
impl From<UnsupportedOSPlatformGuardAttribute> for System::Runtime::Versioning::OSPlatformAttribute {
 fn from(v:UnsupportedOSPlatformGuardAttribute)->System::Runtime::Versioning::OSPlatformAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Versioning::OSPlatformAttribute,UnsupportedOSPlatformGuardAttribute>(v)
}} 
impl UnsupportedOSPlatformGuardAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type RequiresPreviewFeaturesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.RequiresPreviewFeaturesAttribute">;
use super::super::super::*;
impl From<RequiresPreviewFeaturesAttribute> for System::Attribute {
 fn from(v:RequiresPreviewFeaturesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiresPreviewFeaturesAttribute>(v)
}} 
impl RequiresPreviewFeaturesAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ResourceConsumptionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.ResourceConsumptionAttribute">;
use super::super::super::*;
impl From<ResourceConsumptionAttribute> for System::Attribute {
 fn from(v:ResourceConsumptionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ResourceConsumptionAttribute>(v)
}} 
pub type ResourceExposureAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.ResourceExposureAttribute">;
use super::super::super::*;
impl From<ResourceExposureAttribute> for System::Attribute {
 fn from(v:ResourceExposureAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ResourceExposureAttribute>(v)
}} 
pub type TargetFrameworkAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.TargetFrameworkAttribute">;
use super::super::super::*;
impl From<TargetFrameworkAttribute> for System::Attribute {
 fn from(v:TargetFrameworkAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TargetFrameworkAttribute>(v)
}} 
impl TargetFrameworkAttribute {
    pub fn get_framework_name(self) -> System::String { self.instance0::<"get_FrameworkName", System::String>() }
    pub fn get_framework_display_name(self) -> System::String { self.instance0::<"get_FrameworkDisplayName", System::String>() }
    pub fn set_framework_display_name(self, a1: System::String) { self.instance1::<"set_FrameworkDisplayName", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type VersioningHelper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Versioning.VersioningHelper">;
use super::super::super::*;
impl From<VersioningHelper> for System::Object {
 fn from(v:VersioningHelper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,VersioningHelper>(v)
}} 
}
pub mod Loader{
pub type AssemblyLoadContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Loader.AssemblyLoadContext">;
use super::super::super::*;
impl From<AssemblyLoadContext> for System::Object {
 fn from(v:AssemblyLoadContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AssemblyLoadContext>(v)
}} 
impl AssemblyLoadContext {
    pub fn get_load_context(a1: System::Reflection::Assembly) -> System::Runtime::Loader::AssemblyLoadContext { Self::static1::<"GetLoadContext", System::Reflection::Assembly, System::Runtime::Loader::AssemblyLoadContext>(a1) }
    pub fn set_profile_optimization_root(self, a1: System::String) { self.instance1::<"SetProfileOptimizationRoot", System::String, ()>(a1) }
    pub fn start_profile_optimization(self, a1: System::String) { self.instance1::<"StartProfileOptimization", System::String, ()>(a1) }
    pub fn get_default() -> System::Runtime::Loader::AssemblyLoadContext { Self::static0::<"get_Default", System::Runtime::Loader::AssemblyLoadContext>() }
    pub fn get_is_collectible(self) -> bool { self.instance0::<"get_IsCollectible", bool>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_assembly_name(a1: System::String) -> System::Reflection::AssemblyName { Self::static1::<"GetAssemblyName", System::String, System::Reflection::AssemblyName>(a1) }
    pub fn load_from_assembly_name(self, a1: System::Reflection::AssemblyName) -> System::Reflection::Assembly { self.instance1::<"LoadFromAssemblyName", System::Reflection::AssemblyName, System::Reflection::Assembly>(a1) }
    pub fn load_from_assembly_path(self, a1: System::String) -> System::Reflection::Assembly { self.instance1::<"LoadFromAssemblyPath", System::String, System::Reflection::Assembly>(a1) }
    pub fn load_from_native_image_path(self, a1: System::String, a2: System::String) -> System::Reflection::Assembly { self.instance2::<"LoadFromNativeImagePath", System::String, System::String, System::Reflection::Assembly>(a1, a2) }
    pub fn load_from_stream(self, a1: System::IO::Stream) -> System::Reflection::Assembly { self.instance1::<"LoadFromStream", System::IO::Stream, System::Reflection::Assembly>(a1) }
    pub fn unload(self) { self.instance0::<"Unload", ()>() }
    pub fn get_current_contextual_reflection_context() -> System::Runtime::Loader::AssemblyLoadContext { Self::static0::<"get_CurrentContextualReflectionContext", System::Runtime::Loader::AssemblyLoadContext>() }
    pub fn new(a1: System::String, a2: bool) -> Self { Self::ctor2(a1, a2) }
}
pub type AssemblyDependencyResolver =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Loader.AssemblyDependencyResolver">;
use super::super::super::*;
impl From<AssemblyDependencyResolver> for System::Object {
 fn from(v:AssemblyDependencyResolver)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AssemblyDependencyResolver>(v)
}} 
impl AssemblyDependencyResolver {
    pub fn resolve_assembly_to_path(self, a1: System::Reflection::AssemblyName) -> System::String { self.instance1::<"ResolveAssemblyToPath", System::Reflection::AssemblyName, System::String>(a1) }
    pub fn resolve_unmanaged_dll_to_path(self, a1: System::String) -> System::String { self.instance1::<"ResolveUnmanagedDllToPath", System::String, System::String>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
}
pub mod Intrinsics{
pub mod Wasm{
pub type PackedSimd =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Wasm.PackedSimd">;
use super::super::super::super::*;
impl From<PackedSimd> for System::Object {
 fn from(v:PackedSimd)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PackedSimd>(v)
}} 
impl PackedSimd {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
}
pub mod Arm{
pub type AdvSimd =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.AdvSimd">;
use super::super::super::super::*;
impl From<AdvSimd> for System::Runtime::Intrinsics::Arm::ArmBase {
 fn from(v:AdvSimd)->System::Runtime::Intrinsics::Arm::ArmBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::ArmBase,AdvSimd>(v)
}} 
impl AdvSimd {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Aes =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Aes">;
use super::super::super::super::*;
impl From<Aes> for System::Runtime::Intrinsics::Arm::ArmBase {
 fn from(v:Aes)->System::Runtime::Intrinsics::Arm::ArmBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::ArmBase,Aes>(v)
}} 
impl Aes {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type ArmBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.ArmBase">;
use super::super::super::super::*;
impl From<ArmBase> for System::Object {
 fn from(v:ArmBase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ArmBase>(v)
}} 
impl ArmBase {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn leading_zero_count(a1: i32) -> i32 { Self::static1::<"LeadingZeroCount", i32, i32>(a1) }
    pub fn reverse_element_bits(a1: i32) -> i32 { Self::static1::<"ReverseElementBits", i32, i32>(a1) }
    pub fn r#yield() { Self::static0::<"Yield", ()>() }
}
pub type Crc32 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Crc32">;
use super::super::super::super::*;
impl From<Crc32> for System::Runtime::Intrinsics::Arm::ArmBase {
 fn from(v:Crc32)->System::Runtime::Intrinsics::Arm::ArmBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::ArmBase,Crc32>(v)
}} 
impl Crc32 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn compute_crc32(a1: u32, a2: u8) -> u32 { Self::static2::<"ComputeCrc32", u32, u8, u32>(a1, a2) }
    pub fn compute_crc32_c(a1: u32, a2: u8) -> u32 { Self::static2::<"ComputeCrc32C", u32, u8, u32>(a1, a2) }
}
pub type Dp =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Dp">;
use super::super::super::super::*;
impl From<Dp> for System::Runtime::Intrinsics::Arm::AdvSimd {
 fn from(v:Dp)->System::Runtime::Intrinsics::Arm::AdvSimd{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::AdvSimd,Dp>(v)
}} 
impl Dp {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Rdm =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Rdm">;
use super::super::super::super::*;
impl From<Rdm> for System::Runtime::Intrinsics::Arm::AdvSimd {
 fn from(v:Rdm)->System::Runtime::Intrinsics::Arm::AdvSimd{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::AdvSimd,Rdm>(v)
}} 
impl Rdm {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Sha1 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Sha1">;
use super::super::super::super::*;
impl From<Sha1> for System::Runtime::Intrinsics::Arm::ArmBase {
 fn from(v:Sha1)->System::Runtime::Intrinsics::Arm::ArmBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::ArmBase,Sha1>(v)
}} 
impl Sha1 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Sha256 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Arm.Sha256">;
use super::super::super::super::*;
impl From<Sha256> for System::Runtime::Intrinsics::Arm::ArmBase {
 fn from(v:Sha256)->System::Runtime::Intrinsics::Arm::ArmBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::Arm::ArmBase,Sha256>(v)
}} 
impl Sha256 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
}
pub mod X86{
pub type X86Base =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.X86Base">;
use super::super::super::super::*;
impl From<X86Base> for System::Object {
 fn from(v:X86Base)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,X86Base>(v)
}} 
impl X86Base {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn pause() { Self::static0::<"Pause", ()>() }
}
pub type Aes =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Aes">;
use super::super::super::super::*;
impl From<Aes> for System::Runtime::Intrinsics::X86::Sse2 {
 fn from(v:Aes)->System::Runtime::Intrinsics::X86::Sse2{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse2,Aes>(v)
}} 
impl Aes {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx">;
use super::super::super::super::*;
impl From<Avx> for System::Runtime::Intrinsics::X86::Sse42 {
 fn from(v:Avx)->System::Runtime::Intrinsics::X86::Sse42{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse42,Avx>(v)
}} 
impl Avx {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx2 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx2">;
use super::super::super::super::*;
impl From<Avx2> for System::Runtime::Intrinsics::X86::Avx {
 fn from(v:Avx2)->System::Runtime::Intrinsics::X86::Avx{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx,Avx2>(v)
}} 
impl Avx2 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx512BW =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx512BW">;
use super::super::super::super::*;
impl From<Avx512BW> for System::Runtime::Intrinsics::X86::Avx512F {
 fn from(v:Avx512BW)->System::Runtime::Intrinsics::X86::Avx512F{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx512F,Avx512BW>(v)
}} 
impl Avx512BW {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx512CD =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx512CD">;
use super::super::super::super::*;
impl From<Avx512CD> for System::Runtime::Intrinsics::X86::Avx512F {
 fn from(v:Avx512CD)->System::Runtime::Intrinsics::X86::Avx512F{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx512F,Avx512CD>(v)
}} 
impl Avx512CD {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx512DQ =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx512DQ">;
use super::super::super::super::*;
impl From<Avx512DQ> for System::Runtime::Intrinsics::X86::Avx512F {
 fn from(v:Avx512DQ)->System::Runtime::Intrinsics::X86::Avx512F{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx512F,Avx512DQ>(v)
}} 
impl Avx512DQ {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx512F =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx512F">;
use super::super::super::super::*;
impl From<Avx512F> for System::Runtime::Intrinsics::X86::Avx2 {
 fn from(v:Avx512F)->System::Runtime::Intrinsics::X86::Avx2{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx2,Avx512F>(v)
}} 
impl Avx512F {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Avx512Vbmi =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Avx512Vbmi">;
use super::super::super::super::*;
impl From<Avx512Vbmi> for System::Runtime::Intrinsics::X86::Avx512BW {
 fn from(v:Avx512Vbmi)->System::Runtime::Intrinsics::X86::Avx512BW{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx512BW,Avx512Vbmi>(v)
}} 
impl Avx512Vbmi {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type AvxVnni =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.AvxVnni">;
use super::super::super::super::*;
impl From<AvxVnni> for System::Runtime::Intrinsics::X86::Avx2 {
 fn from(v:AvxVnni)->System::Runtime::Intrinsics::X86::Avx2{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx2,AvxVnni>(v)
}} 
impl AvxVnni {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Bmi1 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Bmi1">;
use super::super::super::super::*;
impl From<Bmi1> for System::Runtime::Intrinsics::X86::X86Base {
 fn from(v:Bmi1)->System::Runtime::Intrinsics::X86::X86Base{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::X86Base,Bmi1>(v)
}} 
impl Bmi1 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn and_not(a1: u32, a2: u32) -> u32 { Self::static2::<"AndNot", u32, u32, u32>(a1, a2) }
    pub fn bit_field_extract(a1: u32, a2: u16) -> u32 { Self::static2::<"BitFieldExtract", u32, u16, u32>(a1, a2) }
    pub fn extract_lowest_set_bit(a1: u32) -> u32 { Self::static1::<"ExtractLowestSetBit", u32, u32>(a1) }
    pub fn get_mask_up_to_lowest_set_bit(a1: u32) -> u32 { Self::static1::<"GetMaskUpToLowestSetBit", u32, u32>(a1) }
    pub fn reset_lowest_set_bit(a1: u32) -> u32 { Self::static1::<"ResetLowestSetBit", u32, u32>(a1) }
    pub fn trailing_zero_count(a1: u32) -> u32 { Self::static1::<"TrailingZeroCount", u32, u32>(a1) }
}
pub type Bmi2 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Bmi2">;
use super::super::super::super::*;
impl From<Bmi2> for System::Runtime::Intrinsics::X86::X86Base {
 fn from(v:Bmi2)->System::Runtime::Intrinsics::X86::X86Base{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::X86Base,Bmi2>(v)
}} 
impl Bmi2 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn zero_high_bits(a1: u32, a2: u32) -> u32 { Self::static2::<"ZeroHighBits", u32, u32, u32>(a1, a2) }
    pub fn multiply_no_flags(a1: u32, a2: u32) -> u32 { Self::static2::<"MultiplyNoFlags", u32, u32, u32>(a1, a2) }
    pub fn parallel_bit_deposit(a1: u32, a2: u32) -> u32 { Self::static2::<"ParallelBitDeposit", u32, u32, u32>(a1, a2) }
    pub fn parallel_bit_extract(a1: u32, a2: u32) -> u32 { Self::static2::<"ParallelBitExtract", u32, u32, u32>(a1, a2) }
}
pub type Fma =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Fma">;
use super::super::super::super::*;
impl From<Fma> for System::Runtime::Intrinsics::X86::Avx {
 fn from(v:Fma)->System::Runtime::Intrinsics::X86::Avx{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Avx,Fma>(v)
}} 
impl Fma {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Lzcnt =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Lzcnt">;
use super::super::super::super::*;
impl From<Lzcnt> for System::Runtime::Intrinsics::X86::X86Base {
 fn from(v:Lzcnt)->System::Runtime::Intrinsics::X86::X86Base{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::X86Base,Lzcnt>(v)
}} 
impl Lzcnt {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn leading_zero_count(a1: u32) -> u32 { Self::static1::<"LeadingZeroCount", u32, u32>(a1) }
}
pub type Pclmulqdq =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Pclmulqdq">;
use super::super::super::super::*;
impl From<Pclmulqdq> for System::Runtime::Intrinsics::X86::Sse2 {
 fn from(v:Pclmulqdq)->System::Runtime::Intrinsics::X86::Sse2{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse2,Pclmulqdq>(v)
}} 
impl Pclmulqdq {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Popcnt =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Popcnt">;
use super::super::super::super::*;
impl From<Popcnt> for System::Runtime::Intrinsics::X86::Sse42 {
 fn from(v:Popcnt)->System::Runtime::Intrinsics::X86::Sse42{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse42,Popcnt>(v)
}} 
impl Popcnt {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn pop_count(a1: u32) -> u32 { Self::static1::<"PopCount", u32, u32>(a1) }
}
pub type Sse =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Sse">;
use super::super::super::super::*;
impl From<Sse> for System::Runtime::Intrinsics::X86::X86Base {
 fn from(v:Sse)->System::Runtime::Intrinsics::X86::X86Base{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::X86Base,Sse>(v)
}} 
impl Sse {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn store_fence() { Self::static0::<"StoreFence", ()>() }
}
pub type Sse2 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Sse2">;
use super::super::super::super::*;
impl From<Sse2> for System::Runtime::Intrinsics::X86::Sse {
 fn from(v:Sse2)->System::Runtime::Intrinsics::X86::Sse{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse,Sse2>(v)
}} 
impl Sse2 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn load_fence() { Self::static0::<"LoadFence", ()>() }
    pub fn memory_fence() { Self::static0::<"MemoryFence", ()>() }
}
pub type Sse3 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Sse3">;
use super::super::super::super::*;
impl From<Sse3> for System::Runtime::Intrinsics::X86::Sse2 {
 fn from(v:Sse3)->System::Runtime::Intrinsics::X86::Sse2{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse2,Sse3>(v)
}} 
impl Sse3 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Sse41 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Sse41">;
use super::super::super::super::*;
impl From<Sse41> for System::Runtime::Intrinsics::X86::Ssse3 {
 fn from(v:Sse41)->System::Runtime::Intrinsics::X86::Ssse3{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Ssse3,Sse41>(v)
}} 
impl Sse41 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type Sse42 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Sse42">;
use super::super::super::super::*;
impl From<Sse42> for System::Runtime::Intrinsics::X86::Sse41 {
 fn from(v:Sse42)->System::Runtime::Intrinsics::X86::Sse41{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse41,Sse42>(v)
}} 
impl Sse42 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn crc32(a1: u32, a2: u8) -> u32 { Self::static2::<"Crc32", u32, u8, u32>(a1, a2) }
}
pub type Ssse3 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.Ssse3">;
use super::super::super::super::*;
impl From<Ssse3> for System::Runtime::Intrinsics::X86::Sse3 {
 fn from(v:Ssse3)->System::Runtime::Intrinsics::X86::Sse3{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::Sse3,Ssse3>(v)
}} 
impl Ssse3 {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type X86Serialize =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.X86.X86Serialize">;
use super::super::super::super::*;
impl From<X86Serialize> for System::Runtime::Intrinsics::X86::X86Base {
 fn from(v:X86Serialize)->System::Runtime::Intrinsics::X86::X86Base{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::Intrinsics::X86::X86Base,X86Serialize>(v)
}} 
impl X86Serialize {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
    pub fn serialize() { Self::static0::<"Serialize", ()>() }
}
}
pub type Vector128 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Vector128">;
use super::super::super::*;
impl From<Vector128> for System::Object {
 fn from(v:Vector128)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Vector128>(v)
}} 
impl Vector128 {
    pub fn get_is_hardware_accelerated() -> bool { Self::static0::<"get_IsHardwareAccelerated", bool>() }
}
pub type Vector256 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Vector256">;
use super::super::super::*;
impl From<Vector256> for System::Object {
 fn from(v:Vector256)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Vector256>(v)
}} 
impl Vector256 {
    pub fn get_is_hardware_accelerated() -> bool { Self::static0::<"get_IsHardwareAccelerated", bool>() }
}
pub type Vector512 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Vector512">;
use super::super::super::*;
impl From<Vector512> for System::Object {
 fn from(v:Vector512)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Vector512>(v)
}} 
impl Vector512 {
    pub fn get_is_hardware_accelerated() -> bool { Self::static0::<"get_IsHardwareAccelerated", bool>() }
}
pub type Vector64 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.Intrinsics.Vector64">;
use super::super::super::*;
impl From<Vector64> for System::Object {
 fn from(v:Vector64)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Vector64>(v)
}} 
impl Vector64 {
    pub fn get_is_hardware_accelerated() -> bool { Self::static0::<"get_IsHardwareAccelerated", bool>() }
}
}
pub mod InteropServices{
pub mod ObjectiveC{
pub type ObjectiveCMarshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ObjectiveC.ObjectiveCMarshal">;
use super::super::super::super::*;
impl From<ObjectiveCMarshal> for System::Object {
 fn from(v:ObjectiveCMarshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ObjectiveCMarshal>(v)
}} 
impl ObjectiveCMarshal {
    pub fn set_message_send_pending_exception(a1: System::Exception) { Self::static1::<"SetMessageSendPendingException", System::Exception, ()>(a1) }
}
pub type ObjectiveCTrackedTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ObjectiveC.ObjectiveCTrackedTypeAttribute">;
use super::super::super::super::*;
impl From<ObjectiveCTrackedTypeAttribute> for System::Attribute {
 fn from(v:ObjectiveCTrackedTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ObjectiveCTrackedTypeAttribute>(v)
}} 
impl ObjectiveCTrackedTypeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod Marshalling{
pub type AnsiStringMarshaller =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.AnsiStringMarshaller">;
use super::super::super::super::*;
impl From<AnsiStringMarshaller> for System::Object {
 fn from(v:AnsiStringMarshaller)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AnsiStringMarshaller>(v)
}} 
pub type BStrStringMarshaller =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.BStrStringMarshaller">;
use super::super::super::super::*;
impl From<BStrStringMarshaller> for System::Object {
 fn from(v:BStrStringMarshaller)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BStrStringMarshaller>(v)
}} 
pub type ContiguousCollectionMarshallerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.ContiguousCollectionMarshallerAttribute">;
use super::super::super::super::*;
impl From<ContiguousCollectionMarshallerAttribute> for System::Attribute {
 fn from(v:ContiguousCollectionMarshallerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContiguousCollectionMarshallerAttribute>(v)
}} 
impl ContiguousCollectionMarshallerAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CustomMarshallerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.CustomMarshallerAttribute">;
use super::super::super::super::*;
impl From<CustomMarshallerAttribute> for System::Attribute {
 fn from(v:CustomMarshallerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CustomMarshallerAttribute>(v)
}} 
impl CustomMarshallerAttribute {
    pub fn get_managed_type(self) -> System::Type { self.instance0::<"get_ManagedType", System::Type>() }
    pub fn get_marshaller_type(self) -> System::Type { self.instance0::<"get_MarshallerType", System::Type>() }
}
pub type MarshalUsingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.MarshalUsingAttribute">;
use super::super::super::super::*;
impl From<MarshalUsingAttribute> for System::Attribute {
 fn from(v:MarshalUsingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MarshalUsingAttribute>(v)
}} 
impl MarshalUsingAttribute {
    pub fn get_native_type(self) -> System::Type { self.instance0::<"get_NativeType", System::Type>() }
    pub fn get_count_element_name(self) -> System::String { self.instance0::<"get_CountElementName", System::String>() }
    pub fn set_count_element_name(self, a1: System::String) { self.instance1::<"set_CountElementName", System::String, ()>(a1) }
    pub fn get_constant_element_count(self) -> i32 { self.instance0::<"get_ConstantElementCount", i32>() }
    pub fn set_constant_element_count(self, a1: i32) { self.instance1::<"set_ConstantElementCount", i32, ()>(a1) }
    pub fn get_element_indirection_depth(self) -> i32 { self.instance0::<"get_ElementIndirectionDepth", i32>() }
    pub fn set_element_indirection_depth(self, a1: i32) { self.instance1::<"set_ElementIndirectionDepth", i32, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type NativeMarshallingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.NativeMarshallingAttribute">;
use super::super::super::super::*;
impl From<NativeMarshallingAttribute> for System::Attribute {
 fn from(v:NativeMarshallingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NativeMarshallingAttribute>(v)
}} 
impl NativeMarshallingAttribute {
    pub fn get_native_type(self) -> System::Type { self.instance0::<"get_NativeType", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type Utf16StringMarshaller =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.Utf16StringMarshaller">;
use super::super::super::super::*;
impl From<Utf16StringMarshaller> for System::Object {
 fn from(v:Utf16StringMarshaller)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Utf16StringMarshaller>(v)
}} 
pub type Utf8StringMarshaller =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshalling.Utf8StringMarshaller">;
use super::super::super::super::*;
impl From<Utf8StringMarshaller> for System::Object {
 fn from(v:Utf8StringMarshaller)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Utf8StringMarshaller>(v)
}} 
pub type ComObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.ComObject">;
use super::super::super::super::*;
impl From<ComObject> for System::Object {
 fn from(v:ComObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ComObject>(v)
}} 
impl ComObject {
    pub fn final_release(self) { self.instance0::<"FinalRelease", ()>() }
}
pub type ExceptionAsVoidMarshaller =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.ExceptionAsVoidMarshaller">;
use super::super::super::super::*;
impl From<ExceptionAsVoidMarshaller> for System::Object {
 fn from(v:ExceptionAsVoidMarshaller)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExceptionAsVoidMarshaller>(v)
}} 
impl ExceptionAsVoidMarshaller {
    pub fn convert_to_unmanaged(a1: System::Exception) { Self::static1::<"ConvertToUnmanaged", System::Exception, ()>(a1) }
}
pub type GeneratedComClassAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.GeneratedComClassAttribute">;
use super::super::super::super::*;
impl From<GeneratedComClassAttribute> for System::Attribute {
 fn from(v:GeneratedComClassAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,GeneratedComClassAttribute>(v)
}} 
impl GeneratedComClassAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type GeneratedComInterfaceAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.GeneratedComInterfaceAttribute">;
use super::super::super::super::*;
impl From<GeneratedComInterfaceAttribute> for System::Attribute {
 fn from(v:GeneratedComInterfaceAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,GeneratedComInterfaceAttribute>(v)
}} 
impl GeneratedComInterfaceAttribute {
    pub fn get_string_marshalling_custom_type(self) -> System::Type { self.instance0::<"get_StringMarshallingCustomType", System::Type>() }
    pub fn set_string_marshalling_custom_type(self, a1: System::Type) { self.instance1::<"set_StringMarshallingCustomType", System::Type, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IComExposedClass =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IComExposedClass">;
use super::super::super::super::*;
pub type IComExposedDetails =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IComExposedDetails">;
use super::super::super::super::*;
pub type IIUnknownCacheStrategy =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IIUnknownCacheStrategy">;
use super::super::super::super::*;
pub type IIUnknownDerivedDetails =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IIUnknownDerivedDetails">;
use super::super::super::super::*;
impl IIUnknownDerivedDetails {
    pub fn get_implementation(self) -> System::Type { self.virt0::<"get_Implementation", System::Type>() }
}
pub type IIUnknownInterfaceDetailsStrategy =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IIUnknownInterfaceDetailsStrategy">;
use super::super::super::super::*;
pub type IIUnknownInterfaceType =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IIUnknownInterfaceType">;
use super::super::super::super::*;
pub type IIUnknownStrategy =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IIUnknownStrategy">;
use super::super::super::super::*;
pub type IUnmanagedVirtualMethodTableProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.IUnmanagedVirtualMethodTableProvider">;
use super::super::super::super::*;
pub type StrategyBasedComWrappers =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.Marshalling.StrategyBasedComWrappers">;
use super::super::super::super::*;
impl From<StrategyBasedComWrappers> for System::Runtime::InteropServices::ComWrappers {
 fn from(v:StrategyBasedComWrappers)->System::Runtime::InteropServices::ComWrappers{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::ComWrappers,StrategyBasedComWrappers>(v)
}} 
impl StrategyBasedComWrappers {
    pub fn get_default_iunknown_interface_details_strategy() -> System::Runtime::InteropServices::Marshalling::IIUnknownInterfaceDetailsStrategy { Self::static0::<"get_DefaultIUnknownInterfaceDetailsStrategy", System::Runtime::InteropServices::Marshalling::IIUnknownInterfaceDetailsStrategy>() }
    pub fn get_default_iunknown_strategy() -> System::Runtime::InteropServices::Marshalling::IIUnknownStrategy { Self::static0::<"get_DefaultIUnknownStrategy", System::Runtime::InteropServices::Marshalling::IIUnknownStrategy>() }
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod ComTypes{
pub type IBindCtx =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IBindCtx">;
use super::super::super::super::*;
impl IBindCtx {
    pub fn release_bound_objects(self) { self.virt0::<"ReleaseBoundObjects", ()>() }
}
pub type IConnectionPoint =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IConnectionPoint">;
use super::super::super::super::*;
pub type IConnectionPointContainer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IConnectionPointContainer">;
use super::super::super::super::*;
pub type IEnumConnectionPoints =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IEnumConnectionPoints">;
use super::super::super::super::*;
impl IEnumConnectionPoints {
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type IEnumConnections =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IEnumConnections">;
use super::super::super::super::*;
impl IEnumConnections {
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type IEnumMoniker =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IEnumMoniker">;
use super::super::super::super::*;
impl IEnumMoniker {
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type IEnumString =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IEnumString">;
use super::super::super::super::*;
impl IEnumString {
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type IEnumVARIANT =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IEnumVARIANT">;
use super::super::super::super::*;
impl IEnumVARIANT {
    pub fn reset(self) -> i32 { self.virt0::<"Reset", i32>() }
    pub fn clone(self) -> System::Runtime::InteropServices::ComTypes::IEnumVARIANT { self.virt0::<"Clone", System::Runtime::InteropServices::ComTypes::IEnumVARIANT>() }
}
pub type IMoniker =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IMoniker">;
use super::super::super::super::*;
impl IMoniker {
    pub fn is_dirty(self) -> i32 { self.virt0::<"IsDirty", i32>() }
}
pub type IPersistFile =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IPersistFile">;
use super::super::super::super::*;
impl IPersistFile {
    pub fn is_dirty(self) -> i32 { self.virt0::<"IsDirty", i32>() }
}
pub type IRunningObjectTable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IRunningObjectTable">;
use super::super::super::super::*;
pub type IStream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.IStream">;
use super::super::super::super::*;
impl IStream {
    pub fn revert(self) { self.virt0::<"Revert", ()>() }
}
pub type ITypeComp =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.ITypeComp">;
use super::super::super::super::*;
pub type ITypeInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.ITypeInfo">;
use super::super::super::super::*;
pub type ITypeInfo2 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.ITypeInfo2">;
use super::super::super::super::*;
pub type ITypeLib =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.ITypeLib">;
use super::super::super::super::*;
impl ITypeLib {
    pub fn get_type_info_count(self) -> i32 { self.virt0::<"GetTypeInfoCount", i32>() }
}
pub type ITypeLib2 =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComTypes.ITypeLib2">;
use super::super::super::super::*;
impl ITypeLib2 {
    pub fn get_type_info_count(self) -> i32 { self.virt0::<"GetTypeInfoCount", i32>() }
}
pub type IAdviseSink =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComTypes.IAdviseSink">;
use super::super::super::super::*;
impl IAdviseSink {
    pub fn on_save(self) { self.virt0::<"OnSave", ()>() }
    pub fn on_close(self) { self.virt0::<"OnClose", ()>() }
}
pub type IDataObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComTypes.IDataObject">;
use super::super::super::super::*;
pub type IEnumFORMATETC =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComTypes.IEnumFORMATETC">;
use super::super::super::super::*;
impl IEnumFORMATETC {
    pub fn reset(self) -> i32 { self.virt0::<"Reset", i32>() }
}
pub type IEnumSTATDATA =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComTypes.IEnumSTATDATA">;
use super::super::super::super::*;
impl IEnumSTATDATA {
    pub fn reset(self) -> i32 { self.virt0::<"Reset", i32>() }
}
}
pub type Marshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.Marshal">;
use super::super::super::*;
impl From<Marshal> for System::Object {
 fn from(v:Marshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Marshal>(v)
}} 
impl Marshal {
    pub fn offset_of(a1: System::Type, a2: System::String) -> isize { Self::static2::<"OffsetOf", System::Type, System::String, isize>(a1, a2) }
    pub fn read_byte(a1: System::Object, a2: i32) -> u8 { Self::static2::<"ReadByte", System::Object, i32, u8>(a1, a2) }
    pub fn read_int16(a1: System::Object, a2: i32) -> i16 { Self::static2::<"ReadInt16", System::Object, i32, i16>(a1, a2) }
    pub fn read_int32(a1: System::Object, a2: i32) -> i32 { Self::static2::<"ReadInt32", System::Object, i32, i32>(a1, a2) }
    pub fn read_int64(a1: System::Object, a2: i32) -> i64 { Self::static2::<"ReadInt64", System::Object, i32, i64>(a1, a2) }
    pub fn get_last_pinvoke_error() -> i32 { Self::static0::<"GetLastPInvokeError", i32>() }
    pub fn set_last_pinvoke_error(a1: i32) { Self::static1::<"SetLastPInvokeError", i32, ()>(a1) }
    pub fn get_exception_pointers() -> isize { Self::static0::<"GetExceptionPointers", isize>() }
    pub fn get_exception_code() -> i32 { Self::static0::<"GetExceptionCode", i32>() }
    pub fn destroy_structure(a1: isize, a2: System::Type) { Self::static2::<"DestroyStructure", isize, System::Type, ()>(a1, a2) }
    pub fn alloc_hglobal(a1: i32) -> isize { Self::static1::<"AllocHGlobal", i32, isize>(a1) }
    pub fn ptr_to_string_ansi(a1: isize) -> System::String { Self::static1::<"PtrToStringAnsi", isize, System::String>(a1) }
    pub fn ptr_to_string_uni(a1: isize) -> System::String { Self::static1::<"PtrToStringUni", isize, System::String>(a1) }
    pub fn ptr_to_string_utf8(a1: isize) -> System::String { Self::static1::<"PtrToStringUTF8", isize, System::String>(a1) }
    pub fn size_of(a1: System::Object) -> i32 { Self::static1::<"SizeOf", System::Object, i32>(a1) }
    pub fn add_ref(a1: isize) -> i32 { Self::static1::<"AddRef", isize, i32>(a1) }
    pub fn release(a1: isize) -> i32 { Self::static1::<"Release", isize, i32>(a1) }
    pub fn unsafe_addr_of_pinned_array_element(a1: System::Array, a2: i32) -> isize { Self::static2::<"UnsafeAddrOfPinnedArrayElement", System::Array, i32, isize>(a1, a2) }
    pub fn read_int_ptr(a1: System::Object, a2: i32) -> isize { Self::static2::<"ReadIntPtr", System::Object, i32, isize>(a1, a2) }
    pub fn write_byte(a1: isize, a2: u8) { Self::static2::<"WriteByte", isize, u8, ()>(a1, a2) }
    pub fn write_int16(a1: isize, a2: i16) { Self::static2::<"WriteInt16", isize, i16, ()>(a1, a2) }
    pub fn write_int32(a1: isize, a2: i32) { Self::static2::<"WriteInt32", isize, i32, ()>(a1, a2) }
    pub fn write_int_ptr(a1: isize, a2: isize) { Self::static2::<"WriteIntPtr", isize, isize, ()>(a1, a2) }
    pub fn write_int64(a1: isize, a2: i64) { Self::static2::<"WriteInt64", isize, i64, ()>(a1, a2) }
    pub fn prelink(a1: System::Reflection::MethodInfo) { Self::static1::<"Prelink", System::Reflection::MethodInfo, ()>(a1) }
    pub fn prelink_all(a1: System::Type) { Self::static1::<"PrelinkAll", System::Type, ()>(a1) }
    pub fn ptr_to_structure(a1: isize, a2: System::Type) -> System::Object { Self::static2::<"PtrToStructure", isize, System::Type, System::Object>(a1, a2) }
    pub fn get_hinstance(a1: System::Reflection::Module) -> isize { Self::static1::<"GetHINSTANCE", System::Reflection::Module, isize>(a1) }
    pub fn get_exception_for_hr(a1: i32) -> System::Exception { Self::static1::<"GetExceptionForHR", i32, System::Exception>(a1) }
    pub fn throw_exception_for_hr(a1: i32) { Self::static1::<"ThrowExceptionForHR", i32, ()>(a1) }
    pub fn secure_string_to_bstr(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToBSTR", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_co_task_mem_ansi(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToCoTaskMemAnsi", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_co_task_mem_unicode(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToCoTaskMemUnicode", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_global_alloc_ansi(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToGlobalAllocAnsi", System::Security::SecureString, isize>(a1) }
    pub fn secure_string_to_global_alloc_unicode(a1: System::Security::SecureString) -> isize { Self::static1::<"SecureStringToGlobalAllocUnicode", System::Security::SecureString, isize>(a1) }
    pub fn string_to_hglobal_ansi(a1: System::String) -> isize { Self::static1::<"StringToHGlobalAnsi", System::String, isize>(a1) }
    pub fn string_to_hglobal_uni(a1: System::String) -> isize { Self::static1::<"StringToHGlobalUni", System::String, isize>(a1) }
    pub fn string_to_co_task_mem_uni(a1: System::String) -> isize { Self::static1::<"StringToCoTaskMemUni", System::String, isize>(a1) }
    pub fn string_to_co_task_mem_utf8(a1: System::String) -> isize { Self::static1::<"StringToCoTaskMemUTF8", System::String, isize>(a1) }
    pub fn string_to_co_task_mem_ansi(a1: System::String) -> isize { Self::static1::<"StringToCoTaskMemAnsi", System::String, isize>(a1) }
    pub fn generate_prog_id_for_type(a1: System::Type) -> System::String { Self::static1::<"GenerateProgIdForType", System::Type, System::String>(a1) }
    pub fn get_delegate_for_function_pointer(a1: isize, a2: System::Type) -> System::Delegate { Self::static2::<"GetDelegateForFunctionPointer", isize, System::Type, System::Delegate>(a1, a2) }
    pub fn get_function_pointer_for_delegate(a1: System::Delegate) -> isize { Self::static1::<"GetFunctionPointerForDelegate", System::Delegate, isize>(a1) }
    pub fn get_hrfor_last_win32_error() -> i32 { Self::static0::<"GetHRForLastWin32Error", i32>() }
    pub fn zero_free_bstr(a1: isize) { Self::static1::<"ZeroFreeBSTR", isize, ()>(a1) }
    pub fn zero_free_co_task_mem_ansi(a1: isize) { Self::static1::<"ZeroFreeCoTaskMemAnsi", isize, ()>(a1) }
    pub fn zero_free_co_task_mem_unicode(a1: isize) { Self::static1::<"ZeroFreeCoTaskMemUnicode", isize, ()>(a1) }
    pub fn zero_free_co_task_mem_utf8(a1: isize) { Self::static1::<"ZeroFreeCoTaskMemUTF8", isize, ()>(a1) }
    pub fn zero_free_global_alloc_ansi(a1: isize) { Self::static1::<"ZeroFreeGlobalAllocAnsi", isize, ()>(a1) }
    pub fn zero_free_global_alloc_unicode(a1: isize) { Self::static1::<"ZeroFreeGlobalAllocUnicode", isize, ()>(a1) }
    pub fn string_to_bstr(a1: System::String) -> isize { Self::static1::<"StringToBSTR", System::String, isize>(a1) }
    pub fn ptr_to_string_bstr(a1: isize) -> System::String { Self::static1::<"PtrToStringBSTR", isize, System::String>(a1) }
    pub fn init_handle(a1: System::Runtime::InteropServices::SafeHandle, a2: isize) { Self::static2::<"InitHandle", System::Runtime::InteropServices::SafeHandle, isize, ()>(a1, a2) }
    pub fn get_last_win32_error() -> i32 { Self::static0::<"GetLastWin32Error", i32>() }
    pub fn get_last_pinvoke_error_message() -> System::String { Self::static0::<"GetLastPInvokeErrorMessage", System::String>() }
    pub fn get_hrfor_exception(a1: System::Exception) -> i32 { Self::static1::<"GetHRForException", System::Exception, i32>(a1) }
    pub fn are_com_objects_available_for_cleanup() -> bool { Self::static0::<"AreComObjectsAvailableForCleanup", bool>() }
    pub fn create_aggregated_object(a1: isize, a2: System::Object) -> isize { Self::static2::<"CreateAggregatedObject", isize, System::Object, isize>(a1, a2) }
    pub fn bind_to_moniker(a1: System::String) -> System::Object { Self::static1::<"BindToMoniker", System::String, System::Object>(a1) }
    pub fn cleanup_unused_objects_in_current_context() { Self::static0::<"CleanupUnusedObjectsInCurrentContext", ()>() }
    pub fn create_wrapper_of_type(a1: System::Object, a2: System::Type) -> System::Object { Self::static2::<"CreateWrapperOfType", System::Object, System::Type, System::Object>(a1, a2) }
    pub fn change_wrapper_handle_strength(a1: System::Object, a2: bool) { Self::static2::<"ChangeWrapperHandleStrength", System::Object, bool, ()>(a1, a2) }
    pub fn final_release_com_object(a1: System::Object) -> i32 { Self::static1::<"FinalReleaseComObject", System::Object, i32>(a1) }
    pub fn get_com_interface_for_object(a1: System::Object, a2: System::Type) -> isize { Self::static2::<"GetComInterfaceForObject", System::Object, System::Type, isize>(a1, a2) }
    pub fn get_com_object_data(a1: System::Object, a2: System::Object) -> System::Object { Self::static2::<"GetComObjectData", System::Object, System::Object, System::Object>(a1, a2) }
    pub fn get_idispatch_for_object(a1: System::Object) -> isize { Self::static1::<"GetIDispatchForObject", System::Object, isize>(a1) }
    pub fn get_iunknown_for_object(a1: System::Object) -> isize { Self::static1::<"GetIUnknownForObject", System::Object, isize>(a1) }
    pub fn get_native_variant_for_object(a1: System::Object, a2: isize) { Self::static2::<"GetNativeVariantForObject", System::Object, isize, ()>(a1, a2) }
    pub fn get_typed_object_for_iunknown(a1: isize, a2: System::Type) -> System::Object { Self::static2::<"GetTypedObjectForIUnknown", isize, System::Type, System::Object>(a1, a2) }
    pub fn get_object_for_iunknown(a1: isize) -> System::Object { Self::static1::<"GetObjectForIUnknown", isize, System::Object>(a1) }
    pub fn get_object_for_native_variant(a1: isize) -> System::Object { Self::static1::<"GetObjectForNativeVariant", isize, System::Object>(a1) }
    pub fn get_start_com_slot(a1: System::Type) -> i32 { Self::static1::<"GetStartComSlot", System::Type, i32>(a1) }
    pub fn get_end_com_slot(a1: System::Type) -> i32 { Self::static1::<"GetEndComSlot", System::Type, i32>(a1) }
    pub fn get_type_info_name(a1: System::Runtime::InteropServices::ComTypes::ITypeInfo) -> System::String { Self::static1::<"GetTypeInfoName", System::Runtime::InteropServices::ComTypes::ITypeInfo, System::String>(a1) }
    pub fn get_unique_object_for_iunknown(a1: isize) -> System::Object { Self::static1::<"GetUniqueObjectForIUnknown", isize, System::Object>(a1) }
    pub fn is_com_object(a1: System::Object) -> bool { Self::static1::<"IsComObject", System::Object, bool>(a1) }
    pub fn is_type_visible_from_com(a1: System::Type) -> bool { Self::static1::<"IsTypeVisibleFromCom", System::Type, bool>(a1) }
    pub fn release_com_object(a1: System::Object) -> i32 { Self::static1::<"ReleaseComObject", System::Object, i32>(a1) }
    pub fn ptr_to_string_auto(a1: isize, a2: i32) -> System::String { Self::static2::<"PtrToStringAuto", isize, i32, System::String>(a1, a2) }
    pub fn string_to_hglobal_auto(a1: System::String) -> isize { Self::static1::<"StringToHGlobalAuto", System::String, isize>(a1) }
    pub fn string_to_co_task_mem_auto(a1: System::String) -> isize { Self::static1::<"StringToCoTaskMemAuto", System::String, isize>(a1) }
    pub fn free_hglobal(a1: isize) { Self::static1::<"FreeHGlobal", isize, ()>(a1) }
    pub fn re_alloc_hglobal(a1: isize, a2: isize) -> isize { Self::static2::<"ReAllocHGlobal", isize, isize, isize>(a1, a2) }
    pub fn alloc_co_task_mem(a1: i32) -> isize { Self::static1::<"AllocCoTaskMem", i32, isize>(a1) }
    pub fn free_co_task_mem(a1: isize) { Self::static1::<"FreeCoTaskMem", isize, ()>(a1) }
    pub fn re_alloc_co_task_mem(a1: isize, a2: i32) -> isize { Self::static2::<"ReAllocCoTaskMem", isize, i32, isize>(a1, a2) }
    pub fn free_bstr(a1: isize) { Self::static1::<"FreeBSTR", isize, ()>(a1) }
    pub fn get_last_system_error() -> i32 { Self::static0::<"GetLastSystemError", i32>() }
    pub fn set_last_system_error(a1: i32) { Self::static1::<"SetLastSystemError", i32, ()>(a1) }
    pub fn get_pinvoke_error_message(a1: i32) -> System::String { Self::static1::<"GetPInvokeErrorMessage", i32, System::String>(a1) }
}
pub type MemoryMarshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.MemoryMarshal">;
use super::super::super::*;
impl From<MemoryMarshal> for System::Object {
 fn from(v:MemoryMarshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MemoryMarshal>(v)
}} 
pub type NativeLibrary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.NativeLibrary">;
use super::super::super::*;
impl From<NativeLibrary> for System::Object {
 fn from(v:NativeLibrary)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NativeLibrary>(v)
}} 
impl NativeLibrary {
    pub fn load(a1: System::String) -> isize { Self::static1::<"Load", System::String, isize>(a1) }
    pub fn free(a1: isize) { Self::static1::<"Free", isize, ()>(a1) }
    pub fn get_export(a1: isize, a2: System::String) -> isize { Self::static2::<"GetExport", isize, System::String, isize>(a1, a2) }
    pub fn set_dll_import_resolver(a1: System::Reflection::Assembly, a2: System::Runtime::InteropServices::DllImportResolver) { Self::static2::<"SetDllImportResolver", System::Reflection::Assembly, System::Runtime::InteropServices::DllImportResolver, ()>(a1, a2) }
    pub fn get_main_program_handle() -> isize { Self::static0::<"GetMainProgramHandle", isize>() }
}
pub type ComWrappers =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComWrappers">;
use super::super::super::*;
impl From<ComWrappers> for System::Object {
 fn from(v:ComWrappers)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ComWrappers>(v)
}} 
impl ComWrappers {
    pub fn register_for_tracker_support(a1: System::Runtime::InteropServices::ComWrappers) { Self::static1::<"RegisterForTrackerSupport", System::Runtime::InteropServices::ComWrappers, ()>(a1) }
    pub fn register_for_marshalling(a1: System::Runtime::InteropServices::ComWrappers) { Self::static1::<"RegisterForMarshalling", System::Runtime::InteropServices::ComWrappers, ()>(a1) }
}
pub type AllowReversePInvokeCallsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.AllowReversePInvokeCallsAttribute">;
use super::super::super::*;
impl From<AllowReversePInvokeCallsAttribute> for System::Attribute {
 fn from(v:AllowReversePInvokeCallsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AllowReversePInvokeCallsAttribute>(v)
}} 
impl AllowReversePInvokeCallsAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type BestFitMappingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.BestFitMappingAttribute">;
use super::super::super::*;
impl From<BestFitMappingAttribute> for System::Attribute {
 fn from(v:BestFitMappingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,BestFitMappingAttribute>(v)
}} 
impl BestFitMappingAttribute {
    pub fn get_best_fit_mapping(self) -> bool { self.instance0::<"get_BestFitMapping", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type BStrWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.BStrWrapper">;
use super::super::super::*;
impl From<BStrWrapper> for System::Object {
 fn from(v:BStrWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BStrWrapper>(v)
}} 
impl BStrWrapper {
    pub fn get_wrapped_object(self) -> System::String { self.instance0::<"get_WrappedObject", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ClassInterfaceAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ClassInterfaceAttribute">;
use super::super::super::*;
impl From<ClassInterfaceAttribute> for System::Attribute {
 fn from(v:ClassInterfaceAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ClassInterfaceAttribute>(v)
}} 
impl ClassInterfaceAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type CoClassAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.CoClassAttribute">;
use super::super::super::*;
impl From<CoClassAttribute> for System::Attribute {
 fn from(v:CoClassAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CoClassAttribute>(v)
}} 
impl CoClassAttribute {
    pub fn get_co_class(self) -> System::Type { self.instance0::<"get_CoClass", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type CollectionsMarshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.CollectionsMarshal">;
use super::super::super::*;
impl From<CollectionsMarshal> for System::Object {
 fn from(v:CollectionsMarshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CollectionsMarshal>(v)
}} 
pub type ComDefaultInterfaceAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComDefaultInterfaceAttribute">;
use super::super::super::*;
impl From<ComDefaultInterfaceAttribute> for System::Attribute {
 fn from(v:ComDefaultInterfaceAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComDefaultInterfaceAttribute>(v)
}} 
impl ComDefaultInterfaceAttribute {
    pub fn get_value(self) -> System::Type { self.instance0::<"get_Value", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type ComEventInterfaceAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComEventInterfaceAttribute">;
use super::super::super::*;
impl From<ComEventInterfaceAttribute> for System::Attribute {
 fn from(v:ComEventInterfaceAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComEventInterfaceAttribute>(v)
}} 
impl ComEventInterfaceAttribute {
    pub fn get_source_interface(self) -> System::Type { self.instance0::<"get_SourceInterface", System::Type>() }
    pub fn get_event_provider(self) -> System::Type { self.instance0::<"get_EventProvider", System::Type>() }
    pub fn new(a1: System::Type, a2: System::Type) -> Self { Self::ctor2(a1, a2) }
}
pub type COMException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.COMException">;
use super::super::super::*;
impl From<COMException> for System::Runtime::InteropServices::ExternalException {
 fn from(v:COMException)->System::Runtime::InteropServices::ExternalException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::ExternalException,COMException>(v)
}} 
impl COMException {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ComImportAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComImportAttribute">;
use super::super::super::*;
impl From<ComImportAttribute> for System::Attribute {
 fn from(v:ComImportAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComImportAttribute>(v)
}} 
impl ComImportAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ComSourceInterfacesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComSourceInterfacesAttribute">;
use super::super::super::*;
impl From<ComSourceInterfacesAttribute> for System::Attribute {
 fn from(v:ComSourceInterfacesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComSourceInterfacesAttribute>(v)
}} 
impl ComSourceInterfacesAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ComVisibleAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComVisibleAttribute">;
use super::super::super::*;
impl From<ComVisibleAttribute> for System::Attribute {
 fn from(v:ComVisibleAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComVisibleAttribute>(v)
}} 
impl ComVisibleAttribute {
    pub fn get_value(self) -> bool { self.instance0::<"get_Value", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type CriticalHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.CriticalHandle">;
use super::super::super::*;
impl From<CriticalHandle> for System::Runtime::ConstrainedExecution::CriticalFinalizerObject {
 fn from(v:CriticalHandle)->System::Runtime::ConstrainedExecution::CriticalFinalizerObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::ConstrainedExecution::CriticalFinalizerObject,CriticalHandle>(v)
}} 
impl CriticalHandle {
    pub fn get_is_closed(self) -> bool { self.instance0::<"get_IsClosed", bool>() }
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
    pub fn close(self) { self.instance0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn set_handle_as_invalid(self) { self.instance0::<"SetHandleAsInvalid", ()>() }
}
pub type CurrencyWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.CurrencyWrapper">;
use super::super::super::*;
impl From<CurrencyWrapper> for System::Object {
 fn from(v:CurrencyWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CurrencyWrapper>(v)
}} 
impl CurrencyWrapper {
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type DefaultCharSetAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DefaultCharSetAttribute">;
use super::super::super::*;
impl From<DefaultCharSetAttribute> for System::Attribute {
 fn from(v:DefaultCharSetAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultCharSetAttribute>(v)
}} 
pub type DefaultDllImportSearchPathsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DefaultDllImportSearchPathsAttribute">;
use super::super::super::*;
impl From<DefaultDllImportSearchPathsAttribute> for System::Attribute {
 fn from(v:DefaultDllImportSearchPathsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultDllImportSearchPathsAttribute>(v)
}} 
pub type DefaultParameterValueAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DefaultParameterValueAttribute">;
use super::super::super::*;
impl From<DefaultParameterValueAttribute> for System::Attribute {
 fn from(v:DefaultParameterValueAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultParameterValueAttribute>(v)
}} 
impl DefaultParameterValueAttribute {
    pub fn get_value(self) -> System::Object { self.instance0::<"get_Value", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type DispatchWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DispatchWrapper">;
use super::super::super::*;
impl From<DispatchWrapper> for System::Object {
 fn from(v:DispatchWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DispatchWrapper>(v)
}} 
impl DispatchWrapper {
    pub fn get_wrapped_object(self) -> System::Object { self.instance0::<"get_WrappedObject", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type DispIdAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DispIdAttribute">;
use super::super::super::*;
impl From<DispIdAttribute> for System::Attribute {
 fn from(v:DispIdAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DispIdAttribute>(v)
}} 
impl DispIdAttribute {
    pub fn get_value(self) -> i32 { self.instance0::<"get_Value", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type DllImportAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DllImportAttribute">;
use super::super::super::*;
impl From<DllImportAttribute> for System::Attribute {
 fn from(v:DllImportAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DllImportAttribute>(v)
}} 
impl DllImportAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ErrorWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ErrorWrapper">;
use super::super::super::*;
impl From<ErrorWrapper> for System::Object {
 fn from(v:ErrorWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ErrorWrapper>(v)
}} 
impl ErrorWrapper {
    pub fn get_error_code(self) -> i32 { self.instance0::<"get_ErrorCode", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type ExternalException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ExternalException">;
use super::super::super::*;
impl From<ExternalException> for System::SystemException {
 fn from(v:ExternalException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ExternalException>(v)
}} 
impl ExternalException {
    pub fn get_error_code(self) -> i32 { self.virt0::<"get_ErrorCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type FieldOffsetAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.FieldOffsetAttribute">;
use super::super::super::*;
impl From<FieldOffsetAttribute> for System::Attribute {
 fn from(v:FieldOffsetAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,FieldOffsetAttribute>(v)
}} 
impl FieldOffsetAttribute {
    pub fn get_value(self) -> i32 { self.instance0::<"get_Value", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type GuidAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.GuidAttribute">;
use super::super::super::*;
impl From<GuidAttribute> for System::Attribute {
 fn from(v:GuidAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,GuidAttribute>(v)
}} 
impl GuidAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ICustomAdapter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ICustomAdapter">;
use super::super::super::*;
impl ICustomAdapter {
    pub fn get_underlying_object(self) -> System::Object { self.virt0::<"GetUnderlyingObject", System::Object>() }
}
pub type ICustomFactory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ICustomFactory">;
use super::super::super::*;
pub type ICustomMarshaler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ICustomMarshaler">;
use super::super::super::*;
impl ICustomMarshaler {
    pub fn get_native_data_size(self) -> i32 { self.virt0::<"GetNativeDataSize", i32>() }
}
pub type ICustomQueryInterface =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ICustomQueryInterface">;
use super::super::super::*;
pub type IDynamicInterfaceCastable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.IDynamicInterfaceCastable">;
use super::super::super::*;
pub type DynamicInterfaceCastableImplementationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DynamicInterfaceCastableImplementationAttribute">;
use super::super::super::*;
impl From<DynamicInterfaceCastableImplementationAttribute> for System::Attribute {
 fn from(v:DynamicInterfaceCastableImplementationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DynamicInterfaceCastableImplementationAttribute>(v)
}} 
impl DynamicInterfaceCastableImplementationAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.InAttribute">;
use super::super::super::*;
impl From<InAttribute> for System::Attribute {
 fn from(v:InAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InAttribute>(v)
}} 
impl InAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InterfaceTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.InterfaceTypeAttribute">;
use super::super::super::*;
impl From<InterfaceTypeAttribute> for System::Attribute {
 fn from(v:InterfaceTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InterfaceTypeAttribute>(v)
}} 
impl InterfaceTypeAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type InvalidComObjectException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.InvalidComObjectException">;
use super::super::super::*;
impl From<InvalidComObjectException> for System::SystemException {
 fn from(v:InvalidComObjectException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidComObjectException>(v)
}} 
impl InvalidComObjectException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidOleVariantTypeException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.InvalidOleVariantTypeException">;
use super::super::super::*;
impl From<InvalidOleVariantTypeException> for System::SystemException {
 fn from(v:InvalidOleVariantTypeException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidOleVariantTypeException>(v)
}} 
impl InvalidOleVariantTypeException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type LCIDConversionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.LCIDConversionAttribute">;
use super::super::super::*;
impl From<LCIDConversionAttribute> for System::Attribute {
 fn from(v:LCIDConversionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,LCIDConversionAttribute>(v)
}} 
impl LCIDConversionAttribute {
    pub fn get_value(self) -> i32 { self.instance0::<"get_Value", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type LibraryImportAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.LibraryImportAttribute">;
use super::super::super::*;
impl From<LibraryImportAttribute> for System::Attribute {
 fn from(v:LibraryImportAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,LibraryImportAttribute>(v)
}} 
impl LibraryImportAttribute {
    pub fn get_library_name(self) -> System::String { self.instance0::<"get_LibraryName", System::String>() }
    pub fn get_entry_point(self) -> System::String { self.instance0::<"get_EntryPoint", System::String>() }
    pub fn set_entry_point(self, a1: System::String) { self.instance1::<"set_EntryPoint", System::String, ()>(a1) }
    pub fn get_string_marshalling_custom_type(self) -> System::Type { self.instance0::<"get_StringMarshallingCustomType", System::Type>() }
    pub fn set_string_marshalling_custom_type(self, a1: System::Type) { self.instance1::<"set_StringMarshallingCustomType", System::Type, ()>(a1) }
    pub fn get_set_last_error(self) -> bool { self.instance0::<"get_SetLastError", bool>() }
    pub fn set_set_last_error(self, a1: bool) { self.instance1::<"set_SetLastError", bool, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type MarshalAsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.MarshalAsAttribute">;
use super::super::super::*;
impl From<MarshalAsAttribute> for System::Attribute {
 fn from(v:MarshalAsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MarshalAsAttribute>(v)
}} 
impl MarshalAsAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type MarshalDirectiveException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.MarshalDirectiveException">;
use super::super::super::*;
impl From<MarshalDirectiveException> for System::SystemException {
 fn from(v:MarshalDirectiveException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,MarshalDirectiveException>(v)
}} 
impl MarshalDirectiveException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DllImportResolver =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.DllImportResolver">;
use super::super::super::*;
impl From<DllImportResolver> for System::MulticastDelegate {
 fn from(v:DllImportResolver)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,DllImportResolver>(v)
}} 
impl DllImportResolver {
    pub fn end_invoke(self, a1: System::IAsyncResult) -> isize { self.instance1::<"EndInvoke", System::IAsyncResult, isize>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type NativeMemory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.NativeMemory">;
use super::super::super::*;
impl From<NativeMemory> for System::Object {
 fn from(v:NativeMemory)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NativeMemory>(v)
}} 
pub type OptionalAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.OptionalAttribute">;
use super::super::super::*;
impl From<OptionalAttribute> for System::Attribute {
 fn from(v:OptionalAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OptionalAttribute>(v)
}} 
impl OptionalAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OutAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.OutAttribute">;
use super::super::super::*;
impl From<OutAttribute> for System::Attribute {
 fn from(v:OutAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,OutAttribute>(v)
}} 
impl OutAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type PosixSignalContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.PosixSignalContext">;
use super::super::super::*;
impl From<PosixSignalContext> for System::Object {
 fn from(v:PosixSignalContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PosixSignalContext>(v)
}} 
impl PosixSignalContext {
    pub fn get_cancel(self) -> bool { self.instance0::<"get_Cancel", bool>() }
    pub fn set_cancel(self, a1: bool) { self.instance1::<"set_Cancel", bool, ()>(a1) }
}
pub type PosixSignalRegistration =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.PosixSignalRegistration">;
use super::super::super::*;
impl From<PosixSignalRegistration> for System::Object {
 fn from(v:PosixSignalRegistration)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,PosixSignalRegistration>(v)
}} 
impl PosixSignalRegistration {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
}
pub type PreserveSigAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.PreserveSigAttribute">;
use super::super::super::*;
impl From<PreserveSigAttribute> for System::Attribute {
 fn from(v:PreserveSigAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,PreserveSigAttribute>(v)
}} 
impl PreserveSigAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ProgIdAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ProgIdAttribute">;
use super::super::super::*;
impl From<ProgIdAttribute> for System::Attribute {
 fn from(v:ProgIdAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ProgIdAttribute>(v)
}} 
impl ProgIdAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type RuntimeInformation =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.RuntimeInformation">;
use super::super::super::*;
impl From<RuntimeInformation> for System::Object {
 fn from(v:RuntimeInformation)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeInformation>(v)
}} 
impl RuntimeInformation {
    pub fn get_framework_description() -> System::String { Self::static0::<"get_FrameworkDescription", System::String>() }
    pub fn get_runtime_identifier() -> System::String { Self::static0::<"get_RuntimeIdentifier", System::String>() }
    pub fn get_osdescription() -> System::String { Self::static0::<"get_OSDescription", System::String>() }
}
pub type SafeArrayRankMismatchException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SafeArrayRankMismatchException">;
use super::super::super::*;
impl From<SafeArrayRankMismatchException> for System::SystemException {
 fn from(v:SafeArrayRankMismatchException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SafeArrayRankMismatchException>(v)
}} 
impl SafeArrayRankMismatchException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SafeArrayTypeMismatchException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SafeArrayTypeMismatchException">;
use super::super::super::*;
impl From<SafeArrayTypeMismatchException> for System::SystemException {
 fn from(v:SafeArrayTypeMismatchException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,SafeArrayTypeMismatchException>(v)
}} 
impl SafeArrayTypeMismatchException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SafeBuffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SafeBuffer">;
use super::super::super::*;
impl From<SafeBuffer> for Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid {
 fn from(v:SafeBuffer)->Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<Microsoft::Win32::SafeHandles::SafeHandleZeroOrMinusOneIsInvalid,SafeBuffer>(v)
}} 
impl SafeBuffer {
    pub fn initialize(self, a1: u64) { self.instance1::<"Initialize", u64, ()>(a1) }
    pub fn release_pointer(self) { self.instance0::<"ReleasePointer", ()>() }
    pub fn get_byte_length(self) -> u64 { self.instance0::<"get_ByteLength", u64>() }
}
pub type SafeHandle =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SafeHandle">;
use super::super::super::*;
impl From<SafeHandle> for System::Runtime::ConstrainedExecution::CriticalFinalizerObject {
 fn from(v:SafeHandle)->System::Runtime::ConstrainedExecution::CriticalFinalizerObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::ConstrainedExecution::CriticalFinalizerObject,SafeHandle>(v)
}} 
impl SafeHandle {
    pub fn dangerous_get_handle(self) -> isize { self.instance0::<"DangerousGetHandle", isize>() }
    pub fn get_is_closed(self) -> bool { self.instance0::<"get_IsClosed", bool>() }
    pub fn get_is_invalid(self) -> bool { self.virt0::<"get_IsInvalid", bool>() }
    pub fn close(self) { self.instance0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn set_handle_as_invalid(self) { self.instance0::<"SetHandleAsInvalid", ()>() }
    pub fn dangerous_release(self) { self.instance0::<"DangerousRelease", ()>() }
}
pub type SEHException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SEHException">;
use super::super::super::*;
impl From<SEHException> for System::Runtime::InteropServices::ExternalException {
 fn from(v:SEHException)->System::Runtime::InteropServices::ExternalException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::InteropServices::ExternalException,SEHException>(v)
}} 
impl SEHException {
    pub fn can_resume(self) -> bool { self.virt0::<"CanResume", bool>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type StructLayoutAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.StructLayoutAttribute">;
use super::super::super::*;
impl From<StructLayoutAttribute> for System::Attribute {
 fn from(v:StructLayoutAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,StructLayoutAttribute>(v)
}} 
impl StructLayoutAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type SuppressGCTransitionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.SuppressGCTransitionAttribute">;
use super::super::super::*;
impl From<SuppressGCTransitionAttribute> for System::Attribute {
 fn from(v:SuppressGCTransitionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SuppressGCTransitionAttribute>(v)
}} 
impl SuppressGCTransitionAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TypeIdentifierAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.TypeIdentifierAttribute">;
use super::super::super::*;
impl From<TypeIdentifierAttribute> for System::Attribute {
 fn from(v:TypeIdentifierAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeIdentifierAttribute>(v)
}} 
impl TypeIdentifierAttribute {
    pub fn get_scope(self) -> System::String { self.instance0::<"get_Scope", System::String>() }
    pub fn get_identifier(self) -> System::String { self.instance0::<"get_Identifier", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnknownWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.UnknownWrapper">;
use super::super::super::*;
impl From<UnknownWrapper> for System::Object {
 fn from(v:UnknownWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,UnknownWrapper>(v)
}} 
impl UnknownWrapper {
    pub fn get_wrapped_object(self) -> System::Object { self.instance0::<"get_WrappedObject", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type UnmanagedCallConvAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.UnmanagedCallConvAttribute">;
use super::super::super::*;
impl From<UnmanagedCallConvAttribute> for System::Attribute {
 fn from(v:UnmanagedCallConvAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnmanagedCallConvAttribute>(v)
}} 
impl UnmanagedCallConvAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnmanagedCallersOnlyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.UnmanagedCallersOnlyAttribute">;
use super::super::super::*;
impl From<UnmanagedCallersOnlyAttribute> for System::Attribute {
 fn from(v:UnmanagedCallersOnlyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnmanagedCallersOnlyAttribute>(v)
}} 
impl UnmanagedCallersOnlyAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnmanagedFunctionPointerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.UnmanagedFunctionPointerAttribute">;
use super::super::super::*;
impl From<UnmanagedFunctionPointerAttribute> for System::Attribute {
 fn from(v:UnmanagedFunctionPointerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnmanagedFunctionPointerAttribute>(v)
}} 
pub type VariantWrapper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.VariantWrapper">;
use super::super::super::*;
impl From<VariantWrapper> for System::Object {
 fn from(v:VariantWrapper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,VariantWrapper>(v)
}} 
impl VariantWrapper {
    pub fn get_wrapped_object(self) -> System::Object { self.instance0::<"get_WrappedObject", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type ComEventsHelper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.ComEventsHelper">;
use super::super::super::*;
impl From<ComEventsHelper> for System::Object {
 fn from(v:ComEventsHelper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ComEventsHelper>(v)
}} 
pub type StandardOleMarshalObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.InteropServices.StandardOleMarshalObject">;
use super::super::super::*;
impl From<StandardOleMarshalObject> for System::MarshalByRefObject {
 fn from(v:StandardOleMarshalObject)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,StandardOleMarshalObject>(v)
}} 
pub type SequenceMarshal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Memory","System.Runtime.InteropServices.SequenceMarshal">;
use super::super::super::*;
impl From<SequenceMarshal> for System::Object {
 fn from(v:SequenceMarshal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SequenceMarshal>(v)
}} 
pub type AutomationProxyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.AutomationProxyAttribute">;
use super::super::super::*;
impl From<AutomationProxyAttribute> for System::Attribute {
 fn from(v:AutomationProxyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AutomationProxyAttribute>(v)
}} 
impl AutomationProxyAttribute {
    pub fn get_value(self) -> bool { self.instance0::<"get_Value", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ComAliasNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComAliasNameAttribute">;
use super::super::super::*;
impl From<ComAliasNameAttribute> for System::Attribute {
 fn from(v:ComAliasNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComAliasNameAttribute>(v)
}} 
impl ComAliasNameAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ComAwareEventInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComAwareEventInfo">;
use super::super::super::*;
impl From<ComAwareEventInfo> for System::Reflection::EventInfo {
 fn from(v:ComAwareEventInfo)->System::Reflection::EventInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::EventInfo,ComAwareEventInfo>(v)
}} 
impl ComAwareEventInfo {
    pub fn add_event_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"AddEventHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn remove_event_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"RemoveEventHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn get_add_method(self, a1: bool) -> System::Reflection::MethodInfo { self.instance1::<"GetAddMethod", bool, System::Reflection::MethodInfo>(a1) }
    pub fn get_raise_method(self, a1: bool) -> System::Reflection::MethodInfo { self.instance1::<"GetRaiseMethod", bool, System::Reflection::MethodInfo>(a1) }
    pub fn get_remove_method(self, a1: bool) -> System::Reflection::MethodInfo { self.instance1::<"GetRemoveMethod", bool, System::Reflection::MethodInfo>(a1) }
    pub fn get_declaring_type(self) -> System::Type { self.virt0::<"get_DeclaringType", System::Type>() }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn get_metadata_token(self) -> i32 { self.virt0::<"get_MetadataToken", i32>() }
    pub fn get_module(self) -> System::Reflection::Module { self.virt0::<"get_Module", System::Reflection::Module>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_reflected_type(self) -> System::Type { self.virt0::<"get_ReflectedType", System::Type>() }
    pub fn new(a1: System::Type, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type ComCompatibleVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComCompatibleVersionAttribute">;
use super::super::super::*;
impl From<ComCompatibleVersionAttribute> for System::Attribute {
 fn from(v:ComCompatibleVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComCompatibleVersionAttribute>(v)
}} 
impl ComCompatibleVersionAttribute {
    pub fn get_major_version(self) -> i32 { self.instance0::<"get_MajorVersion", i32>() }
    pub fn get_minor_version(self) -> i32 { self.instance0::<"get_MinorVersion", i32>() }
    pub fn get_build_number(self) -> i32 { self.instance0::<"get_BuildNumber", i32>() }
    pub fn get_revision_number(self) -> i32 { self.instance0::<"get_RevisionNumber", i32>() }
}
pub type ComConversionLossAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComConversionLossAttribute">;
use super::super::super::*;
impl From<ComConversionLossAttribute> for System::Attribute {
 fn from(v:ComConversionLossAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComConversionLossAttribute>(v)
}} 
impl ComConversionLossAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ComRegisterFunctionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComRegisterFunctionAttribute">;
use super::super::super::*;
impl From<ComRegisterFunctionAttribute> for System::Attribute {
 fn from(v:ComRegisterFunctionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComRegisterFunctionAttribute>(v)
}} 
impl ComRegisterFunctionAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ComUnregisterFunctionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ComUnregisterFunctionAttribute">;
use super::super::super::*;
impl From<ComUnregisterFunctionAttribute> for System::Attribute {
 fn from(v:ComUnregisterFunctionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ComUnregisterFunctionAttribute>(v)
}} 
impl ComUnregisterFunctionAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type HandleCollector =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.HandleCollector">;
use super::super::super::*;
impl From<HandleCollector> for System::Object {
 fn from(v:HandleCollector)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,HandleCollector>(v)
}} 
impl HandleCollector {
    pub fn get_count(self) -> i32 { self.instance0::<"get_Count", i32>() }
    pub fn get_initial_threshold(self) -> i32 { self.instance0::<"get_InitialThreshold", i32>() }
    pub fn get_maximum_threshold(self) -> i32 { self.instance0::<"get_MaximumThreshold", i32>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn add(self) { self.instance0::<"Add", ()>() }
    pub fn remove(self) { self.instance0::<"Remove", ()>() }
    pub fn new(a1: System::String, a2: i32) -> Self { Self::ctor2(a1, a2) }
}
pub type ImportedFromTypeLibAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ImportedFromTypeLibAttribute">;
use super::super::super::*;
impl From<ImportedFromTypeLibAttribute> for System::Attribute {
 fn from(v:ImportedFromTypeLibAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ImportedFromTypeLibAttribute>(v)
}} 
impl ImportedFromTypeLibAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ManagedToNativeComInteropStubAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.ManagedToNativeComInteropStubAttribute">;
use super::super::super::*;
impl From<ManagedToNativeComInteropStubAttribute> for System::Attribute {
 fn from(v:ManagedToNativeComInteropStubAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ManagedToNativeComInteropStubAttribute>(v)
}} 
impl ManagedToNativeComInteropStubAttribute {
    pub fn get_class_type(self) -> System::Type { self.instance0::<"get_ClassType", System::Type>() }
    pub fn get_method_name(self) -> System::String { self.instance0::<"get_MethodName", System::String>() }
    pub fn new(a1: System::Type, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type PrimaryInteropAssemblyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.PrimaryInteropAssemblyAttribute">;
use super::super::super::*;
impl From<PrimaryInteropAssemblyAttribute> for System::Attribute {
 fn from(v:PrimaryInteropAssemblyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,PrimaryInteropAssemblyAttribute>(v)
}} 
impl PrimaryInteropAssemblyAttribute {
    pub fn get_major_version(self) -> i32 { self.instance0::<"get_MajorVersion", i32>() }
    pub fn get_minor_version(self) -> i32 { self.instance0::<"get_MinorVersion", i32>() }
    pub fn new(a1: i32, a2: i32) -> Self { Self::ctor2(a1, a2) }
}
pub type RuntimeEnvironment =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.RuntimeEnvironment">;
use super::super::super::*;
impl From<RuntimeEnvironment> for System::Object {
 fn from(v:RuntimeEnvironment)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeEnvironment>(v)
}} 
impl RuntimeEnvironment {
    pub fn get_system_configuration_file() -> System::String { Self::static0::<"get_SystemConfigurationFile", System::String>() }
    pub fn from_global_access_cache(a1: System::Reflection::Assembly) -> bool { Self::static1::<"FromGlobalAccessCache", System::Reflection::Assembly, bool>(a1) }
    pub fn get_runtime_directory() -> System::String { Self::static0::<"GetRuntimeDirectory", System::String>() }
    pub fn get_system_version() -> System::String { Self::static0::<"GetSystemVersion", System::String>() }
}
pub type TypeLibFuncAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.TypeLibFuncAttribute">;
use super::super::super::*;
impl From<TypeLibFuncAttribute> for System::Attribute {
 fn from(v:TypeLibFuncAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeLibFuncAttribute>(v)
}} 
impl TypeLibFuncAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type TypeLibImportClassAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.TypeLibImportClassAttribute">;
use super::super::super::*;
impl From<TypeLibImportClassAttribute> for System::Attribute {
 fn from(v:TypeLibImportClassAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeLibImportClassAttribute>(v)
}} 
impl TypeLibImportClassAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type TypeLibTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.TypeLibTypeAttribute">;
use super::super::super::*;
impl From<TypeLibTypeAttribute> for System::Attribute {
 fn from(v:TypeLibTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeLibTypeAttribute>(v)
}} 
impl TypeLibTypeAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type TypeLibVarAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.TypeLibVarAttribute">;
use super::super::super::*;
impl From<TypeLibVarAttribute> for System::Attribute {
 fn from(v:TypeLibVarAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeLibVarAttribute>(v)
}} 
impl TypeLibVarAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type TypeLibVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.InteropServices.TypeLibVersionAttribute">;
use super::super::super::*;
impl From<TypeLibVersionAttribute> for System::Attribute {
 fn from(v:TypeLibVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeLibVersionAttribute>(v)
}} 
impl TypeLibVersionAttribute {
    pub fn get_major_version(self) -> i32 { self.instance0::<"get_MajorVersion", i32>() }
    pub fn get_minor_version(self) -> i32 { self.instance0::<"get_MinorVersion", i32>() }
    pub fn new(a1: i32, a2: i32) -> Self { Self::ctor2(a1, a2) }
}
}
pub mod CompilerServices{
pub type RuntimeHelpers =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RuntimeHelpers">;
use super::super::super::*;
impl From<RuntimeHelpers> for System::Object {
 fn from(v:RuntimeHelpers)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeHelpers>(v)
}} 
impl RuntimeHelpers {
    pub fn get_object_value(a1: System::Object) -> System::Object { Self::static1::<"GetObjectValue", System::Object, System::Object>(a1) }
    pub fn prepare_delegate(a1: System::Delegate) { Self::static1::<"PrepareDelegate", System::Delegate, ()>(a1) }
    pub fn get_hash_code(a1: System::Object) -> i32 { Self::static1::<"GetHashCode", System::Object, i32>(a1) }
    pub fn equals(a1: System::Object, a2: System::Object) -> bool { Self::static2::<"Equals", System::Object, System::Object, bool>(a1, a2) }
    pub fn get_offset_to_string_data() -> i32 { Self::static0::<"get_OffsetToStringData", i32>() }
    pub fn ensure_sufficient_execution_stack() { Self::static0::<"EnsureSufficientExecutionStack", ()>() }
    pub fn try_ensure_sufficient_execution_stack() -> bool { Self::static0::<"TryEnsureSufficientExecutionStack", bool>() }
    pub fn get_uninitialized_object(a1: System::Type) -> System::Object { Self::static1::<"GetUninitializedObject", System::Type, System::Object>(a1) }
    pub fn allocate_type_associated_memory(a1: System::Type, a2: i32) -> isize { Self::static2::<"AllocateTypeAssociatedMemory", System::Type, i32, isize>(a1, a2) }
    pub fn prepare_contracted_delegate(a1: System::Delegate) { Self::static1::<"PrepareContractedDelegate", System::Delegate, ()>(a1) }
    pub fn probe_for_sufficient_stack() { Self::static0::<"ProbeForSufficientStack", ()>() }
    pub fn prepare_constrained_regions() { Self::static0::<"PrepareConstrainedRegions", ()>() }
    pub fn prepare_constrained_regions_no_op() { Self::static0::<"PrepareConstrainedRegionsNoOP", ()>() }
}
pub type AccessedThroughPropertyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.AccessedThroughPropertyAttribute">;
use super::super::super::*;
impl From<AccessedThroughPropertyAttribute> for System::Attribute {
 fn from(v:AccessedThroughPropertyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AccessedThroughPropertyAttribute>(v)
}} 
impl AccessedThroughPropertyAttribute {
    pub fn get_property_name(self) -> System::String { self.instance0::<"get_PropertyName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AsyncIteratorStateMachineAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.AsyncIteratorStateMachineAttribute">;
use super::super::super::*;
impl From<AsyncIteratorStateMachineAttribute> for System::Runtime::CompilerServices::StateMachineAttribute {
 fn from(v:AsyncIteratorStateMachineAttribute)->System::Runtime::CompilerServices::StateMachineAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::StateMachineAttribute,AsyncIteratorStateMachineAttribute>(v)
}} 
impl AsyncIteratorStateMachineAttribute {
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type AsyncMethodBuilderAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.AsyncMethodBuilderAttribute">;
use super::super::super::*;
impl From<AsyncMethodBuilderAttribute> for System::Attribute {
 fn from(v:AsyncMethodBuilderAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AsyncMethodBuilderAttribute>(v)
}} 
impl AsyncMethodBuilderAttribute {
    pub fn get_builder_type(self) -> System::Type { self.instance0::<"get_BuilderType", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type AsyncStateMachineAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.AsyncStateMachineAttribute">;
use super::super::super::*;
impl From<AsyncStateMachineAttribute> for System::Runtime::CompilerServices::StateMachineAttribute {
 fn from(v:AsyncStateMachineAttribute)->System::Runtime::CompilerServices::StateMachineAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::StateMachineAttribute,AsyncStateMachineAttribute>(v)
}} 
impl AsyncStateMachineAttribute {
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type CallerArgumentExpressionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallerArgumentExpressionAttribute">;
use super::super::super::*;
impl From<CallerArgumentExpressionAttribute> for System::Attribute {
 fn from(v:CallerArgumentExpressionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CallerArgumentExpressionAttribute>(v)
}} 
impl CallerArgumentExpressionAttribute {
    pub fn get_parameter_name(self) -> System::String { self.instance0::<"get_ParameterName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type CallerFilePathAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallerFilePathAttribute">;
use super::super::super::*;
impl From<CallerFilePathAttribute> for System::Attribute {
 fn from(v:CallerFilePathAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CallerFilePathAttribute>(v)
}} 
impl CallerFilePathAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallerLineNumberAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallerLineNumberAttribute">;
use super::super::super::*;
impl From<CallerLineNumberAttribute> for System::Attribute {
 fn from(v:CallerLineNumberAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CallerLineNumberAttribute>(v)
}} 
impl CallerLineNumberAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallerMemberNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallerMemberNameAttribute">;
use super::super::super::*;
impl From<CallerMemberNameAttribute> for System::Attribute {
 fn from(v:CallerMemberNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CallerMemberNameAttribute>(v)
}} 
impl CallerMemberNameAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvCdecl =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvCdecl">;
use super::super::super::*;
impl From<CallConvCdecl> for System::Object {
 fn from(v:CallConvCdecl)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvCdecl>(v)
}} 
impl CallConvCdecl {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvFastcall =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvFastcall">;
use super::super::super::*;
impl From<CallConvFastcall> for System::Object {
 fn from(v:CallConvFastcall)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvFastcall>(v)
}} 
impl CallConvFastcall {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvStdcall =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvStdcall">;
use super::super::super::*;
impl From<CallConvStdcall> for System::Object {
 fn from(v:CallConvStdcall)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvStdcall>(v)
}} 
impl CallConvStdcall {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvSuppressGCTransition =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvSuppressGCTransition">;
use super::super::super::*;
impl From<CallConvSuppressGCTransition> for System::Object {
 fn from(v:CallConvSuppressGCTransition)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvSuppressGCTransition>(v)
}} 
impl CallConvSuppressGCTransition {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvThiscall =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvThiscall">;
use super::super::super::*;
impl From<CallConvThiscall> for System::Object {
 fn from(v:CallConvThiscall)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvThiscall>(v)
}} 
impl CallConvThiscall {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CallConvMemberFunction =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CallConvMemberFunction">;
use super::super::super::*;
impl From<CallConvMemberFunction> for System::Object {
 fn from(v:CallConvMemberFunction)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallConvMemberFunction>(v)
}} 
impl CallConvMemberFunction {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CollectionBuilderAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CollectionBuilderAttribute">;
use super::super::super::*;
impl From<CollectionBuilderAttribute> for System::Attribute {
 fn from(v:CollectionBuilderAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CollectionBuilderAttribute>(v)
}} 
impl CollectionBuilderAttribute {
    pub fn get_builder_type(self) -> System::Type { self.instance0::<"get_BuilderType", System::Type>() }
    pub fn get_method_name(self) -> System::String { self.instance0::<"get_MethodName", System::String>() }
    pub fn new(a1: System::Type, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type CompilationRelaxationsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CompilationRelaxationsAttribute">;
use super::super::super::*;
impl From<CompilationRelaxationsAttribute> for System::Attribute {
 fn from(v:CompilationRelaxationsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CompilationRelaxationsAttribute>(v)
}} 
impl CompilationRelaxationsAttribute {
    pub fn get_compilation_relaxations(self) -> i32 { self.instance0::<"get_CompilationRelaxations", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type CompilerFeatureRequiredAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CompilerFeatureRequiredAttribute">;
use super::super::super::*;
impl From<CompilerFeatureRequiredAttribute> for System::Attribute {
 fn from(v:CompilerFeatureRequiredAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CompilerFeatureRequiredAttribute>(v)
}} 
impl CompilerFeatureRequiredAttribute {
    pub fn get_feature_name(self) -> System::String { self.instance0::<"get_FeatureName", System::String>() }
    pub fn get_is_optional(self) -> bool { self.instance0::<"get_IsOptional", bool>() }
    pub fn set_is_optional(self, a1: bool) { self.instance1::<"set_IsOptional", bool, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type CompilerGeneratedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CompilerGeneratedAttribute">;
use super::super::super::*;
impl From<CompilerGeneratedAttribute> for System::Attribute {
 fn from(v:CompilerGeneratedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CompilerGeneratedAttribute>(v)
}} 
impl CompilerGeneratedAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CompilerGlobalScopeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CompilerGlobalScopeAttribute">;
use super::super::super::*;
impl From<CompilerGlobalScopeAttribute> for System::Attribute {
 fn from(v:CompilerGlobalScopeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CompilerGlobalScopeAttribute>(v)
}} 
impl CompilerGlobalScopeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractHelper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ContractHelper">;
use super::super::super::*;
impl From<ContractHelper> for System::Object {
 fn from(v:ContractHelper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ContractHelper>(v)
}} 
pub type CreateNewOnMetadataUpdateAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CreateNewOnMetadataUpdateAttribute">;
use super::super::super::*;
impl From<CreateNewOnMetadataUpdateAttribute> for System::Attribute {
 fn from(v:CreateNewOnMetadataUpdateAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CreateNewOnMetadataUpdateAttribute>(v)
}} 
impl CreateNewOnMetadataUpdateAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CustomConstantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.CustomConstantAttribute">;
use super::super::super::*;
impl From<CustomConstantAttribute> for System::Attribute {
 fn from(v:CustomConstantAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CustomConstantAttribute>(v)
}} 
impl CustomConstantAttribute {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
}
pub type DateTimeConstantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DateTimeConstantAttribute">;
use super::super::super::*;
impl From<DateTimeConstantAttribute> for System::Runtime::CompilerServices::CustomConstantAttribute {
 fn from(v:DateTimeConstantAttribute)->System::Runtime::CompilerServices::CustomConstantAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::CustomConstantAttribute,DateTimeConstantAttribute>(v)
}} 
impl DateTimeConstantAttribute {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
    pub fn new(a1: i64) -> Self { Self::ctor1(a1) }
}
pub type DecimalConstantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DecimalConstantAttribute">;
use super::super::super::*;
impl From<DecimalConstantAttribute> for System::Attribute {
 fn from(v:DecimalConstantAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DecimalConstantAttribute>(v)
}} 
pub type DefaultDependencyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DefaultDependencyAttribute">;
use super::super::super::*;
impl From<DefaultDependencyAttribute> for System::Attribute {
 fn from(v:DefaultDependencyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultDependencyAttribute>(v)
}} 
pub type DependencyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DependencyAttribute">;
use super::super::super::*;
impl From<DependencyAttribute> for System::Attribute {
 fn from(v:DependencyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DependencyAttribute>(v)
}} 
impl DependencyAttribute {
    pub fn get_dependent_assembly(self) -> System::String { self.instance0::<"get_DependentAssembly", System::String>() }
}
pub type DisablePrivateReflectionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DisablePrivateReflectionAttribute">;
use super::super::super::*;
impl From<DisablePrivateReflectionAttribute> for System::Attribute {
 fn from(v:DisablePrivateReflectionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DisablePrivateReflectionAttribute>(v)
}} 
impl DisablePrivateReflectionAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DisableRuntimeMarshallingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DisableRuntimeMarshallingAttribute">;
use super::super::super::*;
impl From<DisableRuntimeMarshallingAttribute> for System::Attribute {
 fn from(v:DisableRuntimeMarshallingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DisableRuntimeMarshallingAttribute>(v)
}} 
impl DisableRuntimeMarshallingAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DiscardableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.DiscardableAttribute">;
use super::super::super::*;
impl From<DiscardableAttribute> for System::Attribute {
 fn from(v:DiscardableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DiscardableAttribute>(v)
}} 
impl DiscardableAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EnumeratorCancellationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.EnumeratorCancellationAttribute">;
use super::super::super::*;
impl From<EnumeratorCancellationAttribute> for System::Attribute {
 fn from(v:EnumeratorCancellationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EnumeratorCancellationAttribute>(v)
}} 
impl EnumeratorCancellationAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ExtensionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ExtensionAttribute">;
use super::super::super::*;
impl From<ExtensionAttribute> for System::Attribute {
 fn from(v:ExtensionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ExtensionAttribute>(v)
}} 
impl ExtensionAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FixedAddressValueTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.FixedAddressValueTypeAttribute">;
use super::super::super::*;
impl From<FixedAddressValueTypeAttribute> for System::Attribute {
 fn from(v:FixedAddressValueTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,FixedAddressValueTypeAttribute>(v)
}} 
impl FixedAddressValueTypeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FixedBufferAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.FixedBufferAttribute">;
use super::super::super::*;
impl From<FixedBufferAttribute> for System::Attribute {
 fn from(v:FixedBufferAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,FixedBufferAttribute>(v)
}} 
impl FixedBufferAttribute {
    pub fn get_element_type(self) -> System::Type { self.instance0::<"get_ElementType", System::Type>() }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn new(a1: System::Type, a2: i32) -> Self { Self::ctor2(a1, a2) }
}
pub type FormattableStringFactory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.FormattableStringFactory">;
use super::super::super::*;
impl From<FormattableStringFactory> for System::Object {
 fn from(v:FormattableStringFactory)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,FormattableStringFactory>(v)
}} 
pub type IAsyncStateMachine =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IAsyncStateMachine">;
use super::super::super::*;
impl IAsyncStateMachine {
    pub fn move_next(self) { self.virt0::<"MoveNext", ()>() }
}
pub type ICastable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ICastable">;
use super::super::super::*;
pub type IndexerNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IndexerNameAttribute">;
use super::super::super::*;
impl From<IndexerNameAttribute> for System::Attribute {
 fn from(v:IndexerNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,IndexerNameAttribute>(v)
}} 
impl IndexerNameAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type INotifyCompletion =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.INotifyCompletion">;
use super::super::super::*;
pub type ICriticalNotifyCompletion =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ICriticalNotifyCompletion">;
use super::super::super::*;
pub type InternalsVisibleToAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.InternalsVisibleToAttribute">;
use super::super::super::*;
impl From<InternalsVisibleToAttribute> for System::Attribute {
 fn from(v:InternalsVisibleToAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InternalsVisibleToAttribute>(v)
}} 
impl InternalsVisibleToAttribute {
    pub fn get_assembly_name(self) -> System::String { self.instance0::<"get_AssemblyName", System::String>() }
    pub fn get_all_internals_visible(self) -> bool { self.instance0::<"get_AllInternalsVisible", bool>() }
    pub fn set_all_internals_visible(self, a1: bool) { self.instance1::<"set_AllInternalsVisible", bool, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type IsByRefLikeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsByRefLikeAttribute">;
use super::super::super::*;
impl From<IsByRefLikeAttribute> for System::Attribute {
 fn from(v:IsByRefLikeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,IsByRefLikeAttribute>(v)
}} 
impl IsByRefLikeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InlineArrayAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.InlineArrayAttribute">;
use super::super::super::*;
impl From<InlineArrayAttribute> for System::Attribute {
 fn from(v:InlineArrayAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InlineArrayAttribute>(v)
}} 
impl InlineArrayAttribute {
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type IsConst =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsConst">;
use super::super::super::*;
impl From<IsConst> for System::Object {
 fn from(v:IsConst)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,IsConst>(v)
}} 
pub type IsExternalInit =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsExternalInit">;
use super::super::super::*;
impl From<IsExternalInit> for System::Object {
 fn from(v:IsExternalInit)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,IsExternalInit>(v)
}} 
pub type IsReadOnlyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsReadOnlyAttribute">;
use super::super::super::*;
impl From<IsReadOnlyAttribute> for System::Attribute {
 fn from(v:IsReadOnlyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,IsReadOnlyAttribute>(v)
}} 
impl IsReadOnlyAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IsVolatile =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsVolatile">;
use super::super::super::*;
impl From<IsVolatile> for System::Object {
 fn from(v:IsVolatile)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,IsVolatile>(v)
}} 
pub type InterpolatedStringHandlerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.InterpolatedStringHandlerAttribute">;
use super::super::super::*;
impl From<InterpolatedStringHandlerAttribute> for System::Attribute {
 fn from(v:InterpolatedStringHandlerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InterpolatedStringHandlerAttribute>(v)
}} 
impl InterpolatedStringHandlerAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InterpolatedStringHandlerArgumentAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.InterpolatedStringHandlerArgumentAttribute">;
use super::super::super::*;
impl From<InterpolatedStringHandlerArgumentAttribute> for System::Attribute {
 fn from(v:InterpolatedStringHandlerArgumentAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,InterpolatedStringHandlerArgumentAttribute>(v)
}} 
impl InterpolatedStringHandlerArgumentAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type IsUnmanagedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IsUnmanagedAttribute">;
use super::super::super::*;
impl From<IsUnmanagedAttribute> for System::Attribute {
 fn from(v:IsUnmanagedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,IsUnmanagedAttribute>(v)
}} 
impl IsUnmanagedAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IteratorStateMachineAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IteratorStateMachineAttribute">;
use super::super::super::*;
impl From<IteratorStateMachineAttribute> for System::Runtime::CompilerServices::StateMachineAttribute {
 fn from(v:IteratorStateMachineAttribute)->System::Runtime::CompilerServices::StateMachineAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::StateMachineAttribute,IteratorStateMachineAttribute>(v)
}} 
impl IteratorStateMachineAttribute {
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type ITuple =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ITuple">;
use super::super::super::*;
impl ITuple {
    pub fn get_length(self) -> i32 { self.virt0::<"get_Length", i32>() }
}
pub type MethodImplAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.MethodImplAttribute">;
use super::super::super::*;
impl From<MethodImplAttribute> for System::Attribute {
 fn from(v:MethodImplAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MethodImplAttribute>(v)
}} 
impl MethodImplAttribute {
    pub fn new(a1: i16) -> Self { Self::ctor1(a1) }
}
pub type ModuleInitializerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ModuleInitializerAttribute">;
use super::super::super::*;
impl From<ModuleInitializerAttribute> for System::Attribute {
 fn from(v:ModuleInitializerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ModuleInitializerAttribute>(v)
}} 
impl ModuleInitializerAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MetadataUpdateOriginalTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.MetadataUpdateOriginalTypeAttribute">;
use super::super::super::*;
impl From<MetadataUpdateOriginalTypeAttribute> for System::Attribute {
 fn from(v:MetadataUpdateOriginalTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MetadataUpdateOriginalTypeAttribute>(v)
}} 
impl MetadataUpdateOriginalTypeAttribute {
    pub fn get_original_type(self) -> System::Type { self.instance0::<"get_OriginalType", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type NullableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.NullableAttribute">;
use super::super::super::*;
impl From<NullableAttribute> for System::Attribute {
 fn from(v:NullableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NullableAttribute>(v)
}} 
impl NullableAttribute {
    pub fn new(a1: u8) -> Self { Self::ctor1(a1) }
}
pub type NullableContextAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.NullableContextAttribute">;
use super::super::super::*;
impl From<NullableContextAttribute> for System::Attribute {
 fn from(v:NullableContextAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NullableContextAttribute>(v)
}} 
impl NullableContextAttribute {
    pub fn new(a1: u8) -> Self { Self::ctor1(a1) }
}
pub type NullablePublicOnlyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.NullablePublicOnlyAttribute">;
use super::super::super::*;
impl From<NullablePublicOnlyAttribute> for System::Attribute {
 fn from(v:NullablePublicOnlyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NullablePublicOnlyAttribute>(v)
}} 
impl NullablePublicOnlyAttribute {
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ReferenceAssemblyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ReferenceAssemblyAttribute">;
use super::super::super::*;
impl From<ReferenceAssemblyAttribute> for System::Attribute {
 fn from(v:ReferenceAssemblyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ReferenceAssemblyAttribute>(v)
}} 
impl ReferenceAssemblyAttribute {
    pub fn get_description(self) -> System::String { self.instance0::<"get_Description", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type PreserveBaseOverridesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.PreserveBaseOverridesAttribute">;
use super::super::super::*;
impl From<PreserveBaseOverridesAttribute> for System::Attribute {
 fn from(v:PreserveBaseOverridesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,PreserveBaseOverridesAttribute>(v)
}} 
impl PreserveBaseOverridesAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type RefSafetyRulesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RefSafetyRulesAttribute">;
use super::super::super::*;
impl From<RefSafetyRulesAttribute> for System::Attribute {
 fn from(v:RefSafetyRulesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RefSafetyRulesAttribute>(v)
}} 
impl RefSafetyRulesAttribute {
    pub fn get_version(self) -> i32 { self.instance0::<"get_Version", i32>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type RequiredMemberAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RequiredMemberAttribute">;
use super::super::super::*;
impl From<RequiredMemberAttribute> for System::Attribute {
 fn from(v:RequiredMemberAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiredMemberAttribute>(v)
}} 
impl RequiredMemberAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type RequiresLocationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RequiresLocationAttribute">;
use super::super::super::*;
impl From<RequiresLocationAttribute> for System::Attribute {
 fn from(v:RequiresLocationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiresLocationAttribute>(v)
}} 
impl RequiresLocationAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type RuntimeCompatibilityAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RuntimeCompatibilityAttribute">;
use super::super::super::*;
impl From<RuntimeCompatibilityAttribute> for System::Attribute {
 fn from(v:RuntimeCompatibilityAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RuntimeCompatibilityAttribute>(v)
}} 
impl RuntimeCompatibilityAttribute {
    pub fn get_wrap_non_exception_throws(self) -> bool { self.instance0::<"get_WrapNonExceptionThrows", bool>() }
    pub fn set_wrap_non_exception_throws(self, a1: bool) { self.instance1::<"set_WrapNonExceptionThrows", bool, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type RuntimeFeature =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RuntimeFeature">;
use super::super::super::*;
impl From<RuntimeFeature> for System::Object {
 fn from(v:RuntimeFeature)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeFeature>(v)
}} 
impl RuntimeFeature {
    pub fn is_supported(a1: System::String) -> bool { Self::static1::<"IsSupported", System::String, bool>(a1) }
    pub fn get_is_dynamic_code_supported() -> bool { Self::static0::<"get_IsDynamicCodeSupported", bool>() }
    pub fn get_is_dynamic_code_compiled() -> bool { Self::static0::<"get_IsDynamicCodeCompiled", bool>() }
}
pub type RuntimeWrappedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.RuntimeWrappedException">;
use super::super::super::*;
impl From<RuntimeWrappedException> for System::Exception {
 fn from(v:RuntimeWrappedException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,RuntimeWrappedException>(v)
}} 
impl RuntimeWrappedException {
    pub fn get_wrapped_exception(self) -> System::Object { self.instance0::<"get_WrappedException", System::Object>() }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type ScopedRefAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.ScopedRefAttribute">;
use super::super::super::*;
impl From<ScopedRefAttribute> for System::Attribute {
 fn from(v:ScopedRefAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ScopedRefAttribute>(v)
}} 
impl ScopedRefAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SkipLocalsInitAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.SkipLocalsInitAttribute">;
use super::super::super::*;
impl From<SkipLocalsInitAttribute> for System::Attribute {
 fn from(v:SkipLocalsInitAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SkipLocalsInitAttribute>(v)
}} 
impl SkipLocalsInitAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SpecialNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.SpecialNameAttribute">;
use super::super::super::*;
impl From<SpecialNameAttribute> for System::Attribute {
 fn from(v:SpecialNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SpecialNameAttribute>(v)
}} 
impl SpecialNameAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type StateMachineAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.StateMachineAttribute">;
use super::super::super::*;
impl From<StateMachineAttribute> for System::Attribute {
 fn from(v:StateMachineAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,StateMachineAttribute>(v)
}} 
impl StateMachineAttribute {
    pub fn get_state_machine_type(self) -> System::Type { self.instance0::<"get_StateMachineType", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type StringFreezingAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.StringFreezingAttribute">;
use super::super::super::*;
impl From<StringFreezingAttribute> for System::Attribute {
 fn from(v:StringFreezingAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,StringFreezingAttribute>(v)
}} 
impl StringFreezingAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IStrongBox =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.IStrongBox">;
use super::super::super::*;
impl IStrongBox {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
}
pub type SuppressIldasmAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.SuppressIldasmAttribute">;
use super::super::super::*;
impl From<SuppressIldasmAttribute> for System::Attribute {
 fn from(v:SuppressIldasmAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SuppressIldasmAttribute>(v)
}} 
impl SuppressIldasmAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type SwitchExpressionException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.SwitchExpressionException">;
use super::super::super::*;
impl From<SwitchExpressionException> for System::InvalidOperationException {
 fn from(v:SwitchExpressionException)->System::InvalidOperationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::InvalidOperationException,SwitchExpressionException>(v)
}} 
impl SwitchExpressionException {
    pub fn get_unmatched_value(self) -> System::Object { self.instance0::<"get_UnmatchedValue", System::Object>() }
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TupleElementNamesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.TupleElementNamesAttribute">;
use super::super::super::*;
impl From<TupleElementNamesAttribute> for System::Attribute {
 fn from(v:TupleElementNamesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TupleElementNamesAttribute>(v)
}} 
pub type TypeForwardedFromAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.TypeForwardedFromAttribute">;
use super::super::super::*;
impl From<TypeForwardedFromAttribute> for System::Attribute {
 fn from(v:TypeForwardedFromAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeForwardedFromAttribute>(v)
}} 
impl TypeForwardedFromAttribute {
    pub fn get_assembly_full_name(self) -> System::String { self.instance0::<"get_AssemblyFullName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type TypeForwardedToAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.TypeForwardedToAttribute">;
use super::super::super::*;
impl From<TypeForwardedToAttribute> for System::Attribute {
 fn from(v:TypeForwardedToAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TypeForwardedToAttribute>(v)
}} 
impl TypeForwardedToAttribute {
    pub fn get_destination(self) -> System::Type { self.instance0::<"get_Destination", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type Unsafe =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.Unsafe">;
use super::super::super::*;
impl From<Unsafe> for System::Object {
 fn from(v:Unsafe)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Unsafe>(v)
}} 
pub type UnsafeAccessorAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.UnsafeAccessorAttribute">;
use super::super::super::*;
impl From<UnsafeAccessorAttribute> for System::Attribute {
 fn from(v:UnsafeAccessorAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnsafeAccessorAttribute>(v)
}} 
impl UnsafeAccessorAttribute {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
}
pub type UnsafeValueTypeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.CompilerServices.UnsafeValueTypeAttribute">;
use super::super::super::*;
impl From<UnsafeValueTypeAttribute> for System::Attribute {
 fn from(v:UnsafeValueTypeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnsafeValueTypeAttribute>(v)
}} 
impl UnsafeValueTypeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IRuntimeVariables =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.IRuntimeVariables">;
use super::super::super::*;
impl IRuntimeVariables {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
}
pub type RuntimeOps =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.RuntimeOps">;
use super::super::super::*;
impl From<RuntimeOps> for System::Object {
 fn from(v:RuntimeOps)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeOps>(v)
}} 
impl RuntimeOps {
    pub fn expando_check_version(a1: System::Dynamic::ExpandoObject, a2: System::Object) -> bool { Self::static2::<"ExpandoCheckVersion", System::Dynamic::ExpandoObject, System::Object, bool>(a1, a2) }
    pub fn create_runtime_variables() -> System::Runtime::CompilerServices::IRuntimeVariables { Self::static0::<"CreateRuntimeVariables", System::Runtime::CompilerServices::IRuntimeVariables>() }
}
pub type CallSite =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.CallSite">;
use super::super::super::*;
impl From<CallSite> for System::Object {
 fn from(v:CallSite)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallSite>(v)
}} 
impl CallSite {
    pub fn get_binder(self) -> System::Runtime::CompilerServices::CallSiteBinder { self.instance0::<"get_Binder", System::Runtime::CompilerServices::CallSiteBinder>() }
    pub fn create(a1: System::Type, a2: System::Runtime::CompilerServices::CallSiteBinder) -> System::Runtime::CompilerServices::CallSite { Self::static2::<"Create", System::Type, System::Runtime::CompilerServices::CallSiteBinder, System::Runtime::CompilerServices::CallSite>(a1, a2) }
}
pub type CallSiteBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.CallSiteBinder">;
use super::super::super::*;
impl From<CallSiteBinder> for System::Object {
 fn from(v:CallSiteBinder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallSiteBinder>(v)
}} 
impl CallSiteBinder {
    pub fn get_update_label() -> System::Linq::Expressions::LabelTarget { Self::static0::<"get_UpdateLabel", System::Linq::Expressions::LabelTarget>() }
}
pub type CallSiteOps =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.CallSiteOps">;
use super::super::super::*;
impl From<CallSiteOps> for System::Object {
 fn from(v:CallSiteOps)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallSiteOps>(v)
}} 
impl CallSiteOps {
    pub fn set_not_matched(a1: System::Runtime::CompilerServices::CallSite) -> bool { Self::static1::<"SetNotMatched", System::Runtime::CompilerServices::CallSite, bool>(a1) }
    pub fn get_match(a1: System::Runtime::CompilerServices::CallSite) -> bool { Self::static1::<"GetMatch", System::Runtime::CompilerServices::CallSite, bool>(a1) }
    pub fn clear_match(a1: System::Runtime::CompilerServices::CallSite) { Self::static1::<"ClearMatch", System::Runtime::CompilerServices::CallSite, ()>(a1) }
}
pub type CallSiteHelpers =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.CallSiteHelpers">;
use super::super::super::*;
impl From<CallSiteHelpers> for System::Object {
 fn from(v:CallSiteHelpers)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallSiteHelpers>(v)
}} 
impl CallSiteHelpers {
    pub fn is_internal_frame(a1: System::Reflection::MethodBase) -> bool { Self::static1::<"IsInternalFrame", System::Reflection::MethodBase, bool>(a1) }
}
pub type DynamicAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.DynamicAttribute">;
use super::super::super::*;
impl From<DynamicAttribute> for System::Attribute {
 fn from(v:DynamicAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DynamicAttribute>(v)
}} 
impl DynamicAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DebugInfoGenerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.DebugInfoGenerator">;
use super::super::super::*;
impl From<DebugInfoGenerator> for System::Object {
 fn from(v:DebugInfoGenerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DebugInfoGenerator>(v)
}} 
impl DebugInfoGenerator {
    pub fn create_pdb_generator() -> System::Runtime::CompilerServices::DebugInfoGenerator { Self::static0::<"CreatePdbGenerator", System::Runtime::CompilerServices::DebugInfoGenerator>() }
}
pub type Closure =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Runtime.CompilerServices.Closure">;
use super::super::super::*;
impl From<Closure> for System::Object {
 fn from(v:Closure)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Closure>(v)
}} 
pub type IDispatchConstantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.CompilerServices.IDispatchConstantAttribute">;
use super::super::super::*;
impl From<IDispatchConstantAttribute> for System::Runtime::CompilerServices::CustomConstantAttribute {
 fn from(v:IDispatchConstantAttribute)->System::Runtime::CompilerServices::CustomConstantAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::CustomConstantAttribute,IDispatchConstantAttribute>(v)
}} 
impl IDispatchConstantAttribute {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IUnknownConstantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Runtime.InteropServices","System.Runtime.CompilerServices.IUnknownConstantAttribute">;
use super::super::super::*;
impl From<IUnknownConstantAttribute> for System::Runtime::CompilerServices::CustomConstantAttribute {
 fn from(v:IUnknownConstantAttribute)->System::Runtime::CompilerServices::CustomConstantAttribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::CustomConstantAttribute,IUnknownConstantAttribute>(v)
}} 
impl IUnknownConstantAttribute {
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
    pub fn new() -> Self { Self::ctor0() }
}
}
pub type ControlledExecution =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ControlledExecution">;
use super::super::*;
impl From<ControlledExecution> for System::Object {
 fn from(v:ControlledExecution)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ControlledExecution>(v)
}} 
pub type GCSettings =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.GCSettings">;
use super::super::*;
impl From<GCSettings> for System::Object {
 fn from(v:GCSettings)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,GCSettings>(v)
}} 
impl GCSettings {
    pub fn get_is_server_gc() -> bool { Self::static0::<"get_IsServerGC", bool>() }
}
pub type JitInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.JitInfo">;
use super::super::*;
impl From<JitInfo> for System::Object {
 fn from(v:JitInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,JitInfo>(v)
}} 
impl JitInfo {
    pub fn get_compiled_ilbytes(a1: bool) -> i64 { Self::static1::<"GetCompiledILBytes", bool, i64>(a1) }
    pub fn get_compiled_method_count(a1: bool) -> i64 { Self::static1::<"GetCompiledMethodCount", bool, i64>(a1) }
}
pub type AmbiguousImplementationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.AmbiguousImplementationException">;
use super::super::*;
impl From<AmbiguousImplementationException> for System::Exception {
 fn from(v:AmbiguousImplementationException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,AmbiguousImplementationException>(v)
}} 
impl AmbiguousImplementationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MemoryFailPoint =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.MemoryFailPoint">;
use super::super::*;
impl From<MemoryFailPoint> for System::Runtime::ConstrainedExecution::CriticalFinalizerObject {
 fn from(v:MemoryFailPoint)->System::Runtime::ConstrainedExecution::CriticalFinalizerObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::ConstrainedExecution::CriticalFinalizerObject,MemoryFailPoint>(v)
}} 
impl MemoryFailPoint {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type AssemblyTargetedPatchBandAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.AssemblyTargetedPatchBandAttribute">;
use super::super::*;
impl From<AssemblyTargetedPatchBandAttribute> for System::Attribute {
 fn from(v:AssemblyTargetedPatchBandAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyTargetedPatchBandAttribute>(v)
}} 
impl AssemblyTargetedPatchBandAttribute {
    pub fn get_targeted_patch_band(self) -> System::String { self.instance0::<"get_TargetedPatchBand", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type TargetedPatchingOptOutAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.TargetedPatchingOptOutAttribute">;
use super::super::*;
impl From<TargetedPatchingOptOutAttribute> for System::Attribute {
 fn from(v:TargetedPatchingOptOutAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,TargetedPatchingOptOutAttribute>(v)
}} 
impl TargetedPatchingOptOutAttribute {
    pub fn get_reason(self) -> System::String { self.instance0::<"get_Reason", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ProfileOptimization =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Runtime.ProfileOptimization">;
use super::super::*;
impl From<ProfileOptimization> for System::Object {
 fn from(v:ProfileOptimization)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ProfileOptimization>(v)
}} 
impl ProfileOptimization {
    pub fn set_profile_root(a1: System::String) { Self::static1::<"SetProfileRoot", System::String, ()>(a1) }
    pub fn start_profile(a1: System::String) { Self::static1::<"StartProfile", System::String, ()>(a1) }
}
}
pub mod Resources{
pub type IResourceReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.IResourceReader">;
use super::super::*;
impl IResourceReader {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
}
pub type MissingManifestResourceException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.MissingManifestResourceException">;
use super::super::*;
impl From<MissingManifestResourceException> for System::SystemException {
 fn from(v:MissingManifestResourceException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,MissingManifestResourceException>(v)
}} 
impl MissingManifestResourceException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MissingSatelliteAssemblyException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.MissingSatelliteAssemblyException">;
use super::super::*;
impl From<MissingSatelliteAssemblyException> for System::SystemException {
 fn from(v:MissingSatelliteAssemblyException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,MissingSatelliteAssemblyException>(v)
}} 
impl MissingSatelliteAssemblyException {
    pub fn get_culture_name(self) -> System::String { self.instance0::<"get_CultureName", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type NeutralResourcesLanguageAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.NeutralResourcesLanguageAttribute">;
use super::super::*;
impl From<NeutralResourcesLanguageAttribute> for System::Attribute {
 fn from(v:NeutralResourcesLanguageAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NeutralResourcesLanguageAttribute>(v)
}} 
impl NeutralResourcesLanguageAttribute {
    pub fn get_culture_name(self) -> System::String { self.instance0::<"get_CultureName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ResourceManager =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.ResourceManager">;
use super::super::*;
impl From<ResourceManager> for System::Object {
 fn from(v:ResourceManager)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ResourceManager>(v)
}} 
impl ResourceManager {
    pub fn get_base_name(self) -> System::String { self.virt0::<"get_BaseName", System::String>() }
    pub fn get_ignore_case(self) -> bool { self.virt0::<"get_IgnoreCase", bool>() }
    pub fn set_ignore_case(self, a1: bool) { self.instance1::<"set_IgnoreCase", bool, ()>(a1) }
    pub fn get_resource_set_type(self) -> System::Type { self.virt0::<"get_ResourceSetType", System::Type>() }
    pub fn release_all_resources(self) { self.virt0::<"ReleaseAllResources", ()>() }
    pub fn get_string(self, a1: System::String) -> System::String { self.instance1::<"GetString", System::String, System::String>(a1) }
    pub fn get_object(self, a1: System::String) -> System::Object { self.instance1::<"GetObject", System::String, System::Object>(a1) }
    pub fn get_stream(self, a1: System::String) -> System::IO::UnmanagedMemoryStream { self.instance1::<"GetStream", System::String, System::IO::UnmanagedMemoryStream>(a1) }
    pub fn new(a1: System::String, a2: System::Reflection::Assembly) -> Self { Self::ctor2(a1, a2) }
}
pub type ResourceReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.ResourceReader">;
use super::super::*;
impl From<ResourceReader> for System::Object {
 fn from(v:ResourceReader)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ResourceReader>(v)
}} 
impl ResourceReader {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ResourceSet =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.ResourceSet">;
use super::super::*;
impl From<ResourceSet> for System::Object {
 fn from(v:ResourceSet)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ResourceSet>(v)
}} 
impl ResourceSet {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn get_default_reader(self) -> System::Type { self.virt0::<"GetDefaultReader", System::Type>() }
    pub fn get_default_writer(self) -> System::Type { self.virt0::<"GetDefaultWriter", System::Type>() }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn get_string(self, a1: System::String) -> System::String { self.instance1::<"GetString", System::String, System::String>(a1) }
    pub fn get_object(self, a1: System::String) -> System::Object { self.instance1::<"GetObject", System::String, System::Object>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SatelliteContractVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Resources.SatelliteContractVersionAttribute">;
use super::super::*;
impl From<SatelliteContractVersionAttribute> for System::Attribute {
 fn from(v:SatelliteContractVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SatelliteContractVersionAttribute>(v)
}} 
impl SatelliteContractVersionAttribute {
    pub fn get_version(self) -> System::String { self.instance0::<"get_Version", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
}
pub mod Reflection{
pub mod Metadata{
pub type AssemblyExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Metadata.AssemblyExtensions">;
use super::super::super::*;
impl From<AssemblyExtensions> for System::Object {
 fn from(v:AssemblyExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AssemblyExtensions>(v)
}} 
pub type MetadataUpdater =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Metadata.MetadataUpdater">;
use super::super::super::*;
impl From<MetadataUpdater> for System::Object {
 fn from(v:MetadataUpdater)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MetadataUpdater>(v)
}} 
impl MetadataUpdater {
    pub fn get_is_supported() -> bool { Self::static0::<"get_IsSupported", bool>() }
}
pub type MetadataUpdateHandlerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Metadata.MetadataUpdateHandlerAttribute">;
use super::super::super::*;
impl From<MetadataUpdateHandlerAttribute> for System::Attribute {
 fn from(v:MetadataUpdateHandlerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MetadataUpdateHandlerAttribute>(v)
}} 
impl MetadataUpdateHandlerAttribute {
    pub fn get_handler_type(self) -> System::Type { self.instance0::<"get_HandlerType", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
}
pub mod Emit{
pub type CustomAttributeBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.CustomAttributeBuilder">;
use super::super::super::*;
impl From<CustomAttributeBuilder> for System::Object {
 fn from(v:CustomAttributeBuilder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CustomAttributeBuilder>(v)
}} 
pub type DynamicILInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.DynamicILInfo">;
use super::super::super::*;
impl From<DynamicILInfo> for System::Object {
 fn from(v:DynamicILInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DynamicILInfo>(v)
}} 
impl DynamicILInfo {
    pub fn get_dynamic_method(self) -> System::Reflection::Emit::DynamicMethod { self.instance0::<"get_DynamicMethod", System::Reflection::Emit::DynamicMethod>() }
    pub fn get_token_for(self, a1: System::Reflection::Emit::DynamicMethod) -> i32 { self.instance1::<"GetTokenFor", System::Reflection::Emit::DynamicMethod, i32>(a1) }
}
pub type DynamicMethod =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.DynamicMethod">;
use super::super::super::*;
impl From<DynamicMethod> for System::Reflection::MethodInfo {
 fn from(v:DynamicMethod)->System::Reflection::MethodInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MethodInfo,DynamicMethod>(v)
}} 
impl DynamicMethod {
    pub fn create_delegate(self, a1: System::Type) -> System::Delegate { self.instance1::<"CreateDelegate", System::Type, System::Delegate>(a1) }
    pub fn get_dynamic_ilinfo(self) -> System::Reflection::Emit::DynamicILInfo { self.instance0::<"GetDynamicILInfo", System::Reflection::Emit::DynamicILInfo>() }
    pub fn get_ilgenerator(self, a1: i32) -> System::Reflection::Emit::ILGenerator { self.instance1::<"GetILGenerator", i32, System::Reflection::Emit::ILGenerator>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_declaring_type(self) -> System::Type { self.virt0::<"get_DeclaringType", System::Type>() }
    pub fn get_reflected_type(self) -> System::Type { self.virt0::<"get_ReflectedType", System::Type>() }
    pub fn get_module(self) -> System::Reflection::Module { self.virt0::<"get_Module", System::Reflection::Module>() }
    pub fn get_base_definition(self) -> System::Reflection::MethodInfo { self.virt0::<"GetBaseDefinition", System::Reflection::MethodInfo>() }
    pub fn get_is_security_critical(self) -> bool { self.virt0::<"get_IsSecurityCritical", bool>() }
    pub fn get_is_security_safe_critical(self) -> bool { self.virt0::<"get_IsSecuritySafeCritical", bool>() }
    pub fn get_is_security_transparent(self) -> bool { self.virt0::<"get_IsSecurityTransparent", bool>() }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_return_parameter(self) -> System::Reflection::ParameterInfo { self.virt0::<"get_ReturnParameter", System::Reflection::ParameterInfo>() }
    pub fn get_return_type_custom_attributes(self) -> System::Reflection::ICustomAttributeProvider { self.virt0::<"get_ReturnTypeCustomAttributes", System::Reflection::ICustomAttributeProvider>() }
    pub fn get_init_locals(self) -> bool { self.instance0::<"get_InitLocals", bool>() }
    pub fn set_init_locals(self, a1: bool) { self.instance1::<"set_InitLocals", bool, ()>(a1) }
}
pub type LocalBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.LocalBuilder">;
use super::super::super::*;
impl From<LocalBuilder> for System::Reflection::LocalVariableInfo {
 fn from(v:LocalBuilder)->System::Reflection::LocalVariableInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::LocalVariableInfo,LocalBuilder>(v)
}} 
impl LocalBuilder {
    pub fn get_is_pinned(self) -> bool { self.virt0::<"get_IsPinned", bool>() }
    pub fn get_local_type(self) -> System::Type { self.virt0::<"get_LocalType", System::Type>() }
    pub fn get_local_index(self) -> i32 { self.virt0::<"get_LocalIndex", i32>() }
}
pub type AssemblyBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.AssemblyBuilder">;
use super::super::super::*;
impl From<AssemblyBuilder> for System::Reflection::Assembly {
 fn from(v:AssemblyBuilder)->System::Reflection::Assembly{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::Assembly,AssemblyBuilder>(v)
}} 
impl AssemblyBuilder {
    pub fn define_dynamic_module(self, a1: System::String) -> System::Reflection::Emit::ModuleBuilder { self.instance1::<"DefineDynamicModule", System::String, System::Reflection::Emit::ModuleBuilder>(a1) }
    pub fn get_dynamic_module(self, a1: System::String) -> System::Reflection::Emit::ModuleBuilder { self.instance1::<"GetDynamicModule", System::String, System::Reflection::Emit::ModuleBuilder>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn get_code_base(self) -> System::String { self.virt0::<"get_CodeBase", System::String>() }
    pub fn get_location(self) -> System::String { self.virt0::<"get_Location", System::String>() }
    pub fn get_entry_point(self) -> System::Reflection::MethodInfo { self.virt0::<"get_EntryPoint", System::Reflection::MethodInfo>() }
    pub fn get_is_dynamic(self) -> bool { self.virt0::<"get_IsDynamic", bool>() }
    pub fn get_file(self, a1: System::String) -> System::IO::FileStream { self.instance1::<"GetFile", System::String, System::IO::FileStream>(a1) }
    pub fn get_manifest_resource_info(self, a1: System::String) -> System::Reflection::ManifestResourceInfo { self.instance1::<"GetManifestResourceInfo", System::String, System::Reflection::ManifestResourceInfo>(a1) }
    pub fn get_manifest_resource_stream(self, a1: System::String) -> System::IO::Stream { self.instance1::<"GetManifestResourceStream", System::String, System::IO::Stream>(a1) }
}
pub type TypeBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.TypeBuilder">;
use super::super::super::*;
impl From<TypeBuilder> for System::Reflection::TypeInfo {
 fn from(v:TypeBuilder)->System::Reflection::TypeInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::TypeInfo,TypeBuilder>(v)
}} 
impl TypeBuilder {
    pub fn get_method(a1: System::Type, a2: System::Reflection::MethodInfo) -> System::Reflection::MethodInfo { Self::static2::<"GetMethod", System::Type, System::Reflection::MethodInfo, System::Reflection::MethodInfo>(a1, a2) }
    pub fn get_constructor(a1: System::Type, a2: System::Reflection::ConstructorInfo) -> System::Reflection::ConstructorInfo { Self::static2::<"GetConstructor", System::Type, System::Reflection::ConstructorInfo, System::Reflection::ConstructorInfo>(a1, a2) }
    pub fn get_field(a1: System::Type, a2: System::Reflection::FieldInfo) -> System::Reflection::FieldInfo { Self::static2::<"GetField", System::Type, System::Reflection::FieldInfo, System::Reflection::FieldInfo>(a1, a2) }
    pub fn get_size(self) -> i32 { self.instance0::<"get_Size", i32>() }
    pub fn add_interface_implementation(self, a1: System::Type) { self.instance1::<"AddInterfaceImplementation", System::Type, ()>(a1) }
    pub fn create_type(self) -> System::Type { self.instance0::<"CreateType", System::Type>() }
    pub fn create_type_info(self) -> System::Reflection::TypeInfo { self.instance0::<"CreateTypeInfo", System::Reflection::TypeInfo>() }
    pub fn define_method_override(self, a1: System::Reflection::MethodInfo, a2: System::Reflection::MethodInfo) { self.instance2::<"DefineMethodOverride", System::Reflection::MethodInfo, System::Reflection::MethodInfo, ()>(a1, a2) }
    pub fn define_nested_type(self, a1: System::String) -> System::Reflection::Emit::TypeBuilder { self.instance1::<"DefineNestedType", System::String, System::Reflection::Emit::TypeBuilder>(a1) }
    pub fn define_type_initializer(self) -> System::Reflection::Emit::ConstructorBuilder { self.instance0::<"DefineTypeInitializer", System::Reflection::Emit::ConstructorBuilder>() }
    pub fn is_created(self) -> bool { self.instance0::<"IsCreated", bool>() }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_parent(self, a1: System::Type) { self.instance1::<"SetParent", System::Type, ()>(a1) }
    pub fn make_pointer_type(self) -> System::Type { self.virt0::<"MakePointerType", System::Type>() }
    pub fn make_by_ref_type(self) -> System::Type { self.virt0::<"MakeByRefType", System::Type>() }
    pub fn make_array_type(self) -> System::Type { self.virt0::<"MakeArrayType", System::Type>() }
}
pub type SignatureHelper =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.SignatureHelper">;
use super::super::super::*;
impl From<SignatureHelper> for System::Object {
 fn from(v:SignatureHelper)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SignatureHelper>(v)
}} 
impl SignatureHelper {
    pub fn get_local_var_sig_helper() -> System::Reflection::Emit::SignatureHelper { Self::static0::<"GetLocalVarSigHelper", System::Reflection::Emit::SignatureHelper>() }
    pub fn get_field_sig_helper(a1: System::Reflection::Module) -> System::Reflection::Emit::SignatureHelper { Self::static1::<"GetFieldSigHelper", System::Reflection::Module, System::Reflection::Emit::SignatureHelper>(a1) }
    pub fn add_argument(self, a1: System::Type) { self.instance1::<"AddArgument", System::Type, ()>(a1) }
    pub fn add_sentinel(self) { self.instance0::<"AddSentinel", ()>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ILGenerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.ILGenerator">;
use super::super::super::*;
impl From<ILGenerator> for System::Object {
 fn from(v:ILGenerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ILGenerator>(v)
}} 
impl ILGenerator {
    pub fn end_exception_block(self) { self.virt0::<"EndExceptionBlock", ()>() }
    pub fn begin_except_filter_block(self) { self.virt0::<"BeginExceptFilterBlock", ()>() }
    pub fn begin_fault_block(self) { self.virt0::<"BeginFaultBlock", ()>() }
    pub fn begin_finally_block(self) { self.virt0::<"BeginFinallyBlock", ()>() }
    pub fn throw_exception(self, a1: System::Type) { self.instance1::<"ThrowException", System::Type, ()>(a1) }
    pub fn emit_write_line(self, a1: System::String) { self.instance1::<"EmitWriteLine", System::String, ()>(a1) }
    pub fn declare_local(self, a1: System::Type) -> System::Reflection::Emit::LocalBuilder { self.instance1::<"DeclareLocal", System::Type, System::Reflection::Emit::LocalBuilder>(a1) }
    pub fn begin_scope(self) { self.virt0::<"BeginScope", ()>() }
    pub fn end_scope(self) { self.virt0::<"EndScope", ()>() }
    pub fn get_iloffset(self) -> i32 { self.virt0::<"get_ILOffset", i32>() }
}
pub type ConstructorBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.ConstructorBuilder">;
use super::super::super::*;
impl From<ConstructorBuilder> for System::Reflection::ConstructorInfo {
 fn from(v:ConstructorBuilder)->System::Reflection::ConstructorInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::ConstructorInfo,ConstructorBuilder>(v)
}} 
impl ConstructorBuilder {
    pub fn get_init_locals(self) -> bool { self.instance0::<"get_InitLocals", bool>() }
    pub fn set_init_locals(self, a1: bool) { self.instance1::<"set_InitLocals", bool, ()>(a1) }
    pub fn get_ilgenerator(self) -> System::Reflection::Emit::ILGenerator { self.instance0::<"GetILGenerator", System::Reflection::Emit::ILGenerator>() }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
}
pub type EnumBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.EnumBuilder">;
use super::super::super::*;
impl From<EnumBuilder> for System::Reflection::TypeInfo {
 fn from(v:EnumBuilder)->System::Reflection::TypeInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::TypeInfo,EnumBuilder>(v)
}} 
impl EnumBuilder {
    pub fn get_underlying_field(self) -> System::Reflection::Emit::FieldBuilder { self.instance0::<"get_UnderlyingField", System::Reflection::Emit::FieldBuilder>() }
    pub fn create_type(self) -> System::Type { self.instance0::<"CreateType", System::Type>() }
    pub fn create_type_info(self) -> System::Reflection::TypeInfo { self.instance0::<"CreateTypeInfo", System::Reflection::TypeInfo>() }
    pub fn define_literal(self, a1: System::String, a2: System::Object) -> System::Reflection::Emit::FieldBuilder { self.instance2::<"DefineLiteral", System::String, System::Object, System::Reflection::Emit::FieldBuilder>(a1, a2) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn make_pointer_type(self) -> System::Type { self.virt0::<"MakePointerType", System::Type>() }
    pub fn make_by_ref_type(self) -> System::Type { self.virt0::<"MakeByRefType", System::Type>() }
    pub fn make_array_type(self) -> System::Type { self.virt0::<"MakeArrayType", System::Type>() }
}
pub type EventBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.EventBuilder">;
use super::super::super::*;
impl From<EventBuilder> for System::Object {
 fn from(v:EventBuilder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EventBuilder>(v)
}} 
impl EventBuilder {
    pub fn add_other_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"AddOtherMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
    pub fn set_add_on_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"SetAddOnMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_raise_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"SetRaiseMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
    pub fn set_remove_on_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"SetRemoveOnMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
}
pub type FieldBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.FieldBuilder">;
use super::super::super::*;
impl From<FieldBuilder> for System::Reflection::FieldInfo {
 fn from(v:FieldBuilder)->System::Reflection::FieldInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::FieldInfo,FieldBuilder>(v)
}} 
impl FieldBuilder {
    pub fn set_constant(self, a1: System::Object) { self.instance1::<"SetConstant", System::Object, ()>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_offset(self, a1: i32) { self.instance1::<"SetOffset", i32, ()>(a1) }
}
pub type GenericTypeParameterBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.GenericTypeParameterBuilder">;
use super::super::super::*;
impl From<GenericTypeParameterBuilder> for System::Reflection::TypeInfo {
 fn from(v:GenericTypeParameterBuilder)->System::Reflection::TypeInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::TypeInfo,GenericTypeParameterBuilder>(v)
}} 
impl GenericTypeParameterBuilder {
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_base_type_constraint(self, a1: System::Type) { self.instance1::<"SetBaseTypeConstraint", System::Type, ()>(a1) }
}
pub type MethodBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.MethodBuilder">;
use super::super::super::*;
impl From<MethodBuilder> for System::Reflection::MethodInfo {
 fn from(v:MethodBuilder)->System::Reflection::MethodInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MethodInfo,MethodBuilder>(v)
}} 
impl MethodBuilder {
    pub fn get_init_locals(self) -> bool { self.instance0::<"get_InitLocals", bool>() }
    pub fn set_init_locals(self, a1: bool) { self.instance1::<"set_InitLocals", bool, ()>(a1) }
    pub fn get_ilgenerator(self) -> System::Reflection::Emit::ILGenerator { self.instance0::<"GetILGenerator", System::Reflection::Emit::ILGenerator>() }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_return_type(self, a1: System::Type) { self.instance1::<"SetReturnType", System::Type, ()>(a1) }
}
pub type ModuleBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.ModuleBuilder">;
use super::super::super::*;
impl From<ModuleBuilder> for System::Reflection::Module {
 fn from(v:ModuleBuilder)->System::Reflection::Module{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::Module,ModuleBuilder>(v)
}} 
impl ModuleBuilder {
    pub fn create_global_functions(self) { self.instance0::<"CreateGlobalFunctions", ()>() }
    pub fn define_type(self, a1: System::String) -> System::Reflection::Emit::TypeBuilder { self.instance1::<"DefineType", System::String, System::Reflection::Emit::TypeBuilder>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
}
pub type OpCodes =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.OpCodes">;
use super::super::super::*;
impl From<OpCodes> for System::Object {
 fn from(v:OpCodes)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,OpCodes>(v)
}} 
pub type ParameterBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.ParameterBuilder">;
use super::super::super::*;
impl From<ParameterBuilder> for System::Object {
 fn from(v:ParameterBuilder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ParameterBuilder>(v)
}} 
impl ParameterBuilder {
    pub fn get_attributes(self) -> i32 { self.virt0::<"get_Attributes", i32>() }
    pub fn get_is_in(self) -> bool { self.instance0::<"get_IsIn", bool>() }
    pub fn get_is_optional(self) -> bool { self.instance0::<"get_IsOptional", bool>() }
    pub fn get_is_out(self) -> bool { self.instance0::<"get_IsOut", bool>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_position(self) -> i32 { self.virt0::<"get_Position", i32>() }
    pub fn set_constant(self, a1: System::Object) { self.instance1::<"SetConstant", System::Object, ()>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
}
pub type PropertyBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Emit.PropertyBuilder">;
use super::super::super::*;
impl From<PropertyBuilder> for System::Reflection::PropertyInfo {
 fn from(v:PropertyBuilder)->System::Reflection::PropertyInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::PropertyInfo,PropertyBuilder>(v)
}} 
impl PropertyBuilder {
    pub fn add_other_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"AddOtherMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
    pub fn set_constant(self, a1: System::Object) { self.instance1::<"SetConstant", System::Object, ()>(a1) }
    pub fn set_custom_attribute(self, a1: System::Reflection::Emit::CustomAttributeBuilder) { self.instance1::<"SetCustomAttribute", System::Reflection::Emit::CustomAttributeBuilder, ()>(a1) }
    pub fn set_get_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"SetGetMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
    pub fn set_set_method(self, a1: System::Reflection::Emit::MethodBuilder) { self.instance1::<"SetSetMethod", System::Reflection::Emit::MethodBuilder, ()>(a1) }
}
}
pub type Assembly =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Assembly">;
use super::super::*;
impl From<Assembly> for System::Object {
 fn from(v:Assembly)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Assembly>(v)
}} 
impl Assembly {
    pub fn load(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"Load", System::String, System::Reflection::Assembly>(a1) }
    pub fn load_with_partial_name(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"LoadWithPartialName", System::String, System::Reflection::Assembly>(a1) }
    pub fn get_executing_assembly() -> System::Reflection::Assembly { Self::static0::<"GetExecutingAssembly", System::Reflection::Assembly>() }
    pub fn get_calling_assembly() -> System::Reflection::Assembly { Self::static0::<"GetCallingAssembly", System::Reflection::Assembly>() }
    pub fn get_code_base(self) -> System::String { self.virt0::<"get_CodeBase", System::String>() }
    pub fn get_entry_point(self) -> System::Reflection::MethodInfo { self.virt0::<"get_EntryPoint", System::Reflection::MethodInfo>() }
    pub fn get_full_name(self) -> System::String { self.virt0::<"get_FullName", System::String>() }
    pub fn get_image_runtime_version(self) -> System::String { self.virt0::<"get_ImageRuntimeVersion", System::String>() }
    pub fn get_is_dynamic(self) -> bool { self.virt0::<"get_IsDynamic", bool>() }
    pub fn get_location(self) -> System::String { self.virt0::<"get_Location", System::String>() }
    pub fn get_reflection_only(self) -> bool { self.virt0::<"get_ReflectionOnly", bool>() }
    pub fn get_is_collectible(self) -> bool { self.virt0::<"get_IsCollectible", bool>() }
    pub fn get_manifest_resource_info(self, a1: System::String) -> System::Reflection::ManifestResourceInfo { self.instance1::<"GetManifestResourceInfo", System::String, System::Reflection::ManifestResourceInfo>(a1) }
    pub fn get_manifest_resource_stream(self, a1: System::String) -> System::IO::Stream { self.instance1::<"GetManifestResourceStream", System::String, System::IO::Stream>(a1) }
    pub fn get_is_fully_trusted(self) -> bool { self.instance0::<"get_IsFullyTrusted", bool>() }
    pub fn get_name(self) -> System::Reflection::AssemblyName { self.virt0::<"GetName", System::Reflection::AssemblyName>() }
    pub fn get_type(self, a1: System::String) -> System::Type { self.instance1::<"GetType", System::String, System::Type>(a1) }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn get_escaped_code_base(self) -> System::String { self.virt0::<"get_EscapedCodeBase", System::String>() }
    pub fn create_instance(self, a1: System::String) -> System::Object { self.instance1::<"CreateInstance", System::String, System::Object>(a1) }
    pub fn add_module_resolve(self, a1: System::Reflection::ModuleResolveEventHandler) { self.instance1::<"add_ModuleResolve", System::Reflection::ModuleResolveEventHandler, ()>(a1) }
    pub fn remove_module_resolve(self, a1: System::Reflection::ModuleResolveEventHandler) { self.instance1::<"remove_ModuleResolve", System::Reflection::ModuleResolveEventHandler, ()>(a1) }
    pub fn get_manifest_module(self) -> System::Reflection::Module { self.virt0::<"get_ManifestModule", System::Reflection::Module>() }
    pub fn get_module(self, a1: System::String) -> System::Reflection::Module { self.instance1::<"GetModule", System::String, System::Reflection::Module>(a1) }
    pub fn get_satellite_assembly(self, a1: System::Globalization::CultureInfo) -> System::Reflection::Assembly { self.instance1::<"GetSatelliteAssembly", System::Globalization::CultureInfo, System::Reflection::Assembly>(a1) }
    pub fn get_file(self, a1: System::String) -> System::IO::FileStream { self.instance1::<"GetFile", System::String, System::IO::FileStream>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_global_assembly_cache(self) -> bool { self.virt0::<"get_GlobalAssemblyCache", bool>() }
    pub fn get_host_context(self) -> i64 { self.virt0::<"get_HostContext", i64>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::Assembly, a2: System::Reflection::Assembly) -> bool { Self::static2::<"op_Equality", System::Reflection::Assembly, System::Reflection::Assembly, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::Assembly, a2: System::Reflection::Assembly) -> bool { Self::static2::<"op_Inequality", System::Reflection::Assembly, System::Reflection::Assembly, bool>(a1, a2) }
    pub fn create_qualified_name(a1: System::String, a2: System::String) -> System::String { Self::static2::<"CreateQualifiedName", System::String, System::String, System::String>(a1, a2) }
    pub fn get_assembly(a1: System::Type) -> System::Reflection::Assembly { Self::static1::<"GetAssembly", System::Type, System::Reflection::Assembly>(a1) }
    pub fn set_entry_assembly(a1: System::Reflection::Assembly) { Self::static1::<"SetEntryAssembly", System::Reflection::Assembly, ()>(a1) }
    pub fn get_entry_assembly() -> System::Reflection::Assembly { Self::static0::<"GetEntryAssembly", System::Reflection::Assembly>() }
    pub fn load_file(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"LoadFile", System::String, System::Reflection::Assembly>(a1) }
    pub fn load_from(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"LoadFrom", System::String, System::Reflection::Assembly>(a1) }
    pub fn unsafe_load_from(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"UnsafeLoadFrom", System::String, System::Reflection::Assembly>(a1) }
    pub fn reflection_only_load(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"ReflectionOnlyLoad", System::String, System::Reflection::Assembly>(a1) }
    pub fn reflection_only_load_from(a1: System::String) -> System::Reflection::Assembly { Self::static1::<"ReflectionOnlyLoadFrom", System::String, System::Reflection::Assembly>(a1) }
}
pub type AssemblyName =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyName">;
use super::super::*;
impl From<AssemblyName> for System::Object {
 fn from(v:AssemblyName)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AssemblyName>(v)
}} 
impl AssemblyName {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn get_version(self) -> System::Version { self.instance0::<"get_Version", System::Version>() }
    pub fn set_version(self, a1: System::Version) { self.instance1::<"set_Version", System::Version, ()>(a1) }
    pub fn get_culture_info(self) -> System::Globalization::CultureInfo { self.instance0::<"get_CultureInfo", System::Globalization::CultureInfo>() }
    pub fn set_culture_info(self, a1: System::Globalization::CultureInfo) { self.instance1::<"set_CultureInfo", System::Globalization::CultureInfo, ()>(a1) }
    pub fn get_culture_name(self) -> System::String { self.instance0::<"get_CultureName", System::String>() }
    pub fn set_culture_name(self, a1: System::String) { self.instance1::<"set_CultureName", System::String, ()>(a1) }
    pub fn get_code_base(self) -> System::String { self.instance0::<"get_CodeBase", System::String>() }
    pub fn set_code_base(self, a1: System::String) { self.instance1::<"set_CodeBase", System::String, ()>(a1) }
    pub fn get_escaped_code_base(self) -> System::String { self.instance0::<"get_EscapedCodeBase", System::String>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_assembly_name(a1: System::String) -> System::Reflection::AssemblyName { Self::static1::<"GetAssemblyName", System::String, System::Reflection::AssemblyName>(a1) }
    pub fn get_key_pair(self) -> System::Reflection::StrongNameKeyPair { self.instance0::<"get_KeyPair", System::Reflection::StrongNameKeyPair>() }
    pub fn set_key_pair(self, a1: System::Reflection::StrongNameKeyPair) { self.instance1::<"set_KeyPair", System::Reflection::StrongNameKeyPair, ()>(a1) }
    pub fn get_full_name(self) -> System::String { self.instance0::<"get_FullName", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn on_deserialization(self, a1: System::Object) { self.instance1::<"OnDeserialization", System::Object, ()>(a1) }
    pub fn reference_matches_definition(a1: System::Reflection::AssemblyName, a2: System::Reflection::AssemblyName) -> bool { Self::static2::<"ReferenceMatchesDefinition", System::Reflection::AssemblyName, System::Reflection::AssemblyName, bool>(a1, a2) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ConstructorInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ConstructorInfo">;
use super::super::*;
impl From<ConstructorInfo> for System::Reflection::MethodBase {
 fn from(v:ConstructorInfo)->System::Reflection::MethodBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MethodBase,ConstructorInfo>(v)
}} 
impl ConstructorInfo {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::ConstructorInfo, a2: System::Reflection::ConstructorInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::ConstructorInfo, System::Reflection::ConstructorInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::ConstructorInfo, a2: System::Reflection::ConstructorInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::ConstructorInfo, System::Reflection::ConstructorInfo, bool>(a1, a2) }
}
pub type ConstructorInvoker =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ConstructorInvoker">;
use super::super::*;
impl From<ConstructorInvoker> for System::Object {
 fn from(v:ConstructorInvoker)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ConstructorInvoker>(v)
}} 
impl ConstructorInvoker {
    pub fn create(a1: System::Reflection::ConstructorInfo) -> System::Reflection::ConstructorInvoker { Self::static1::<"Create", System::Reflection::ConstructorInfo, System::Reflection::ConstructorInvoker>(a1) }
    pub fn invoke(self) -> System::Object { self.instance0::<"Invoke", System::Object>() }
}
pub type FieldInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.FieldInfo">;
use super::super::*;
impl From<FieldInfo> for System::Reflection::MemberInfo {
 fn from(v:FieldInfo)->System::Reflection::MemberInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MemberInfo,FieldInfo>(v)
}} 
impl FieldInfo {
    pub fn get_field_type(self) -> System::Type { self.virt0::<"get_FieldType", System::Type>() }
    pub fn get_is_init_only(self) -> bool { self.instance0::<"get_IsInitOnly", bool>() }
    pub fn get_is_literal(self) -> bool { self.instance0::<"get_IsLiteral", bool>() }
    pub fn get_is_not_serialized(self) -> bool { self.instance0::<"get_IsNotSerialized", bool>() }
    pub fn get_is_pinvoke_impl(self) -> bool { self.instance0::<"get_IsPinvokeImpl", bool>() }
    pub fn get_is_special_name(self) -> bool { self.instance0::<"get_IsSpecialName", bool>() }
    pub fn get_is_static(self) -> bool { self.instance0::<"get_IsStatic", bool>() }
    pub fn get_is_assembly(self) -> bool { self.instance0::<"get_IsAssembly", bool>() }
    pub fn get_is_family(self) -> bool { self.instance0::<"get_IsFamily", bool>() }
    pub fn get_is_family_and_assembly(self) -> bool { self.instance0::<"get_IsFamilyAndAssembly", bool>() }
    pub fn get_is_family_or_assembly(self) -> bool { self.instance0::<"get_IsFamilyOrAssembly", bool>() }
    pub fn get_is_private(self) -> bool { self.instance0::<"get_IsPrivate", bool>() }
    pub fn get_is_public(self) -> bool { self.instance0::<"get_IsPublic", bool>() }
    pub fn get_is_security_critical(self) -> bool { self.virt0::<"get_IsSecurityCritical", bool>() }
    pub fn get_is_security_safe_critical(self) -> bool { self.virt0::<"get_IsSecuritySafeCritical", bool>() }
    pub fn get_is_security_transparent(self) -> bool { self.virt0::<"get_IsSecurityTransparent", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::FieldInfo, a2: System::Reflection::FieldInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::FieldInfo, System::Reflection::FieldInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::FieldInfo, a2: System::Reflection::FieldInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::FieldInfo, System::Reflection::FieldInfo, bool>(a1, a2) }
    pub fn set_value(self, a1: System::Object, a2: System::Object) { self.instance2::<"SetValue", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_raw_constant_value(self) -> System::Object { self.virt0::<"GetRawConstantValue", System::Object>() }
    pub fn get_modified_field_type(self) -> System::Type { self.virt0::<"GetModifiedFieldType", System::Type>() }
}
pub type MemberInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MemberInfo">;
use super::super::*;
impl From<MemberInfo> for System::Object {
 fn from(v:MemberInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MemberInfo>(v)
}} 
impl MemberInfo {
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_declaring_type(self) -> System::Type { self.virt0::<"get_DeclaringType", System::Type>() }
    pub fn get_reflected_type(self) -> System::Type { self.virt0::<"get_ReflectedType", System::Type>() }
    pub fn get_module(self) -> System::Reflection::Module { self.virt0::<"get_Module", System::Reflection::Module>() }
    pub fn has_same_metadata_definition_as(self, a1: System::Reflection::MemberInfo) -> bool { self.instance1::<"HasSameMetadataDefinitionAs", System::Reflection::MemberInfo, bool>(a1) }
    pub fn get_is_collectible(self) -> bool { self.virt0::<"get_IsCollectible", bool>() }
    pub fn get_metadata_token(self) -> i32 { self.virt0::<"get_MetadataToken", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::MemberInfo, a2: System::Reflection::MemberInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::MemberInfo, System::Reflection::MemberInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::MemberInfo, a2: System::Reflection::MemberInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::MemberInfo, System::Reflection::MemberInfo, bool>(a1, a2) }
}
pub type MethodBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MethodBase">;
use super::super::*;
impl From<MethodBase> for System::Reflection::MemberInfo {
 fn from(v:MethodBase)->System::Reflection::MemberInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MemberInfo,MethodBase>(v)
}} 
impl MethodBase {
    pub fn get_current_method() -> System::Reflection::MethodBase { Self::static0::<"GetCurrentMethod", System::Reflection::MethodBase>() }
    pub fn get_method_body(self) -> System::Reflection::MethodBody { self.virt0::<"GetMethodBody", System::Reflection::MethodBody>() }
    pub fn get_is_abstract(self) -> bool { self.instance0::<"get_IsAbstract", bool>() }
    pub fn get_is_constructor(self) -> bool { self.instance0::<"get_IsConstructor", bool>() }
    pub fn get_is_final(self) -> bool { self.instance0::<"get_IsFinal", bool>() }
    pub fn get_is_hide_by_sig(self) -> bool { self.instance0::<"get_IsHideBySig", bool>() }
    pub fn get_is_special_name(self) -> bool { self.instance0::<"get_IsSpecialName", bool>() }
    pub fn get_is_static(self) -> bool { self.instance0::<"get_IsStatic", bool>() }
    pub fn get_is_virtual(self) -> bool { self.instance0::<"get_IsVirtual", bool>() }
    pub fn get_is_assembly(self) -> bool { self.instance0::<"get_IsAssembly", bool>() }
    pub fn get_is_family(self) -> bool { self.instance0::<"get_IsFamily", bool>() }
    pub fn get_is_family_and_assembly(self) -> bool { self.instance0::<"get_IsFamilyAndAssembly", bool>() }
    pub fn get_is_family_or_assembly(self) -> bool { self.instance0::<"get_IsFamilyOrAssembly", bool>() }
    pub fn get_is_private(self) -> bool { self.instance0::<"get_IsPrivate", bool>() }
    pub fn get_is_public(self) -> bool { self.instance0::<"get_IsPublic", bool>() }
    pub fn get_is_constructed_generic_method(self) -> bool { self.virt0::<"get_IsConstructedGenericMethod", bool>() }
    pub fn get_is_generic_method(self) -> bool { self.virt0::<"get_IsGenericMethod", bool>() }
    pub fn get_is_generic_method_definition(self) -> bool { self.virt0::<"get_IsGenericMethodDefinition", bool>() }
    pub fn get_contains_generic_parameters(self) -> bool { self.virt0::<"get_ContainsGenericParameters", bool>() }
    pub fn get_is_security_critical(self) -> bool { self.virt0::<"get_IsSecurityCritical", bool>() }
    pub fn get_is_security_safe_critical(self) -> bool { self.virt0::<"get_IsSecuritySafeCritical", bool>() }
    pub fn get_is_security_transparent(self) -> bool { self.virt0::<"get_IsSecurityTransparent", bool>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::MethodBase, a2: System::Reflection::MethodBase) -> bool { Self::static2::<"op_Equality", System::Reflection::MethodBase, System::Reflection::MethodBase, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::MethodBase, a2: System::Reflection::MethodBase) -> bool { Self::static2::<"op_Inequality", System::Reflection::MethodBase, System::Reflection::MethodBase, bool>(a1, a2) }
}
pub type MethodInvoker =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MethodInvoker">;
use super::super::*;
impl From<MethodInvoker> for System::Object {
 fn from(v:MethodInvoker)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MethodInvoker>(v)
}} 
impl MethodInvoker {
    pub fn create(a1: System::Reflection::MethodBase) -> System::Reflection::MethodInvoker { Self::static1::<"Create", System::Reflection::MethodBase, System::Reflection::MethodInvoker>(a1) }
    pub fn invoke(self, a1: System::Object) -> System::Object { self.instance1::<"Invoke", System::Object, System::Object>(a1) }
}
pub type AmbiguousMatchException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AmbiguousMatchException">;
use super::super::*;
impl From<AmbiguousMatchException> for System::SystemException {
 fn from(v:AmbiguousMatchException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,AmbiguousMatchException>(v)
}} 
impl AmbiguousMatchException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type AssemblyAlgorithmIdAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyAlgorithmIdAttribute">;
use super::super::*;
impl From<AssemblyAlgorithmIdAttribute> for System::Attribute {
 fn from(v:AssemblyAlgorithmIdAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyAlgorithmIdAttribute>(v)
}} 
impl AssemblyAlgorithmIdAttribute {
    pub fn get_algorithm_id(self) -> u32 { self.instance0::<"get_AlgorithmId", u32>() }
    pub fn new(a1: u32) -> Self { Self::ctor1(a1) }
}
pub type AssemblyCompanyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyCompanyAttribute">;
use super::super::*;
impl From<AssemblyCompanyAttribute> for System::Attribute {
 fn from(v:AssemblyCompanyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyCompanyAttribute>(v)
}} 
impl AssemblyCompanyAttribute {
    pub fn get_company(self) -> System::String { self.instance0::<"get_Company", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyConfigurationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyConfigurationAttribute">;
use super::super::*;
impl From<AssemblyConfigurationAttribute> for System::Attribute {
 fn from(v:AssemblyConfigurationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyConfigurationAttribute>(v)
}} 
impl AssemblyConfigurationAttribute {
    pub fn get_configuration(self) -> System::String { self.instance0::<"get_Configuration", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyCopyrightAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyCopyrightAttribute">;
use super::super::*;
impl From<AssemblyCopyrightAttribute> for System::Attribute {
 fn from(v:AssemblyCopyrightAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyCopyrightAttribute>(v)
}} 
impl AssemblyCopyrightAttribute {
    pub fn get_copyright(self) -> System::String { self.instance0::<"get_Copyright", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyCultureAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyCultureAttribute">;
use super::super::*;
impl From<AssemblyCultureAttribute> for System::Attribute {
 fn from(v:AssemblyCultureAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyCultureAttribute>(v)
}} 
impl AssemblyCultureAttribute {
    pub fn get_culture(self) -> System::String { self.instance0::<"get_Culture", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyDefaultAliasAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyDefaultAliasAttribute">;
use super::super::*;
impl From<AssemblyDefaultAliasAttribute> for System::Attribute {
 fn from(v:AssemblyDefaultAliasAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyDefaultAliasAttribute>(v)
}} 
impl AssemblyDefaultAliasAttribute {
    pub fn get_default_alias(self) -> System::String { self.instance0::<"get_DefaultAlias", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyDelaySignAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyDelaySignAttribute">;
use super::super::*;
impl From<AssemblyDelaySignAttribute> for System::Attribute {
 fn from(v:AssemblyDelaySignAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyDelaySignAttribute>(v)
}} 
impl AssemblyDelaySignAttribute {
    pub fn get_delay_sign(self) -> bool { self.instance0::<"get_DelaySign", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type AssemblyDescriptionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyDescriptionAttribute">;
use super::super::*;
impl From<AssemblyDescriptionAttribute> for System::Attribute {
 fn from(v:AssemblyDescriptionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyDescriptionAttribute>(v)
}} 
impl AssemblyDescriptionAttribute {
    pub fn get_description(self) -> System::String { self.instance0::<"get_Description", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyFileVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyFileVersionAttribute">;
use super::super::*;
impl From<AssemblyFileVersionAttribute> for System::Attribute {
 fn from(v:AssemblyFileVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyFileVersionAttribute>(v)
}} 
impl AssemblyFileVersionAttribute {
    pub fn get_version(self) -> System::String { self.instance0::<"get_Version", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyFlagsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyFlagsAttribute">;
use super::super::*;
impl From<AssemblyFlagsAttribute> for System::Attribute {
 fn from(v:AssemblyFlagsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyFlagsAttribute>(v)
}} 
impl AssemblyFlagsAttribute {
    pub fn get_flags(self) -> u32 { self.instance0::<"get_Flags", u32>() }
    pub fn get_assembly_flags(self) -> i32 { self.instance0::<"get_AssemblyFlags", i32>() }
    pub fn new(a1: u32) -> Self { Self::ctor1(a1) }
}
pub type AssemblyInformationalVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyInformationalVersionAttribute">;
use super::super::*;
impl From<AssemblyInformationalVersionAttribute> for System::Attribute {
 fn from(v:AssemblyInformationalVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyInformationalVersionAttribute>(v)
}} 
impl AssemblyInformationalVersionAttribute {
    pub fn get_informational_version(self) -> System::String { self.instance0::<"get_InformationalVersion", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyKeyFileAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyKeyFileAttribute">;
use super::super::*;
impl From<AssemblyKeyFileAttribute> for System::Attribute {
 fn from(v:AssemblyKeyFileAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyKeyFileAttribute>(v)
}} 
impl AssemblyKeyFileAttribute {
    pub fn get_key_file(self) -> System::String { self.instance0::<"get_KeyFile", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyKeyNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyKeyNameAttribute">;
use super::super::*;
impl From<AssemblyKeyNameAttribute> for System::Attribute {
 fn from(v:AssemblyKeyNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyKeyNameAttribute>(v)
}} 
impl AssemblyKeyNameAttribute {
    pub fn get_key_name(self) -> System::String { self.instance0::<"get_KeyName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyMetadataAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyMetadataAttribute">;
use super::super::*;
impl From<AssemblyMetadataAttribute> for System::Attribute {
 fn from(v:AssemblyMetadataAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyMetadataAttribute>(v)
}} 
impl AssemblyMetadataAttribute {
    pub fn get_key(self) -> System::String { self.instance0::<"get_Key", System::String>() }
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type AssemblyNameProxy =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyNameProxy">;
use super::super::*;
impl From<AssemblyNameProxy> for System::MarshalByRefObject {
 fn from(v:AssemblyNameProxy)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,AssemblyNameProxy>(v)
}} 
impl AssemblyNameProxy {
    pub fn get_assembly_name(self, a1: System::String) -> System::Reflection::AssemblyName { self.instance1::<"GetAssemblyName", System::String, System::Reflection::AssemblyName>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type AssemblyProductAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyProductAttribute">;
use super::super::*;
impl From<AssemblyProductAttribute> for System::Attribute {
 fn from(v:AssemblyProductAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyProductAttribute>(v)
}} 
impl AssemblyProductAttribute {
    pub fn get_product(self) -> System::String { self.instance0::<"get_Product", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblySignatureKeyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblySignatureKeyAttribute">;
use super::super::*;
impl From<AssemblySignatureKeyAttribute> for System::Attribute {
 fn from(v:AssemblySignatureKeyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblySignatureKeyAttribute>(v)
}} 
impl AssemblySignatureKeyAttribute {
    pub fn get_public_key(self) -> System::String { self.instance0::<"get_PublicKey", System::String>() }
    pub fn get_countersignature(self) -> System::String { self.instance0::<"get_Countersignature", System::String>() }
    pub fn new(a1: System::String, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type AssemblyTitleAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyTitleAttribute">;
use super::super::*;
impl From<AssemblyTitleAttribute> for System::Attribute {
 fn from(v:AssemblyTitleAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyTitleAttribute>(v)
}} 
impl AssemblyTitleAttribute {
    pub fn get_title(self) -> System::String { self.instance0::<"get_Title", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyTrademarkAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyTrademarkAttribute">;
use super::super::*;
impl From<AssemblyTrademarkAttribute> for System::Attribute {
 fn from(v:AssemblyTrademarkAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyTrademarkAttribute>(v)
}} 
impl AssemblyTrademarkAttribute {
    pub fn get_trademark(self) -> System::String { self.instance0::<"get_Trademark", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AssemblyVersionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.AssemblyVersionAttribute">;
use super::super::*;
impl From<AssemblyVersionAttribute> for System::Attribute {
 fn from(v:AssemblyVersionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AssemblyVersionAttribute>(v)
}} 
impl AssemblyVersionAttribute {
    pub fn get_version(self) -> System::String { self.instance0::<"get_Version", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type Binder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Binder">;
use super::super::*;
impl From<Binder> for System::Object {
 fn from(v:Binder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Binder>(v)
}} 
pub type CustomAttributeData =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.CustomAttributeData">;
use super::super::*;
impl From<CustomAttributeData> for System::Object {
 fn from(v:CustomAttributeData)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CustomAttributeData>(v)
}} 
impl CustomAttributeData {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_attribute_type(self) -> System::Type { self.virt0::<"get_AttributeType", System::Type>() }
    pub fn get_constructor(self) -> System::Reflection::ConstructorInfo { self.virt0::<"get_Constructor", System::Reflection::ConstructorInfo>() }
}
pub type CustomAttributeExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.CustomAttributeExtensions">;
use super::super::*;
impl From<CustomAttributeExtensions> for System::Object {
 fn from(v:CustomAttributeExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CustomAttributeExtensions>(v)
}} 
impl CustomAttributeExtensions {
    pub fn get_custom_attribute(a1: System::Reflection::Assembly, a2: System::Type) -> System::Attribute { Self::static2::<"GetCustomAttribute", System::Reflection::Assembly, System::Type, System::Attribute>(a1, a2) }
    pub fn is_defined(a1: System::Reflection::Assembly, a2: System::Type) -> bool { Self::static2::<"IsDefined", System::Reflection::Assembly, System::Type, bool>(a1, a2) }
}
pub type CustomAttributeFormatException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.CustomAttributeFormatException">;
use super::super::*;
impl From<CustomAttributeFormatException> for System::FormatException {
 fn from(v:CustomAttributeFormatException)->System::FormatException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::FormatException,CustomAttributeFormatException>(v)
}} 
impl CustomAttributeFormatException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DefaultMemberAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.DefaultMemberAttribute">;
use super::super::*;
impl From<DefaultMemberAttribute> for System::Attribute {
 fn from(v:DefaultMemberAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DefaultMemberAttribute>(v)
}} 
impl DefaultMemberAttribute {
    pub fn get_member_name(self) -> System::String { self.instance0::<"get_MemberName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type EventInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.EventInfo">;
use super::super::*;
impl From<EventInfo> for System::Reflection::MemberInfo {
 fn from(v:EventInfo)->System::Reflection::MemberInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MemberInfo,EventInfo>(v)
}} 
impl EventInfo {
    pub fn get_is_special_name(self) -> bool { self.instance0::<"get_IsSpecialName", bool>() }
    pub fn get_add_method(self) -> System::Reflection::MethodInfo { self.virt0::<"get_AddMethod", System::Reflection::MethodInfo>() }
    pub fn get_remove_method(self) -> System::Reflection::MethodInfo { self.virt0::<"get_RemoveMethod", System::Reflection::MethodInfo>() }
    pub fn get_raise_method(self) -> System::Reflection::MethodInfo { self.virt0::<"get_RaiseMethod", System::Reflection::MethodInfo>() }
    pub fn get_is_multicast(self) -> bool { self.virt0::<"get_IsMulticast", bool>() }
    pub fn get_event_handler_type(self) -> System::Type { self.virt0::<"get_EventHandlerType", System::Type>() }
    pub fn add_event_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"AddEventHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn remove_event_handler(self, a1: System::Object, a2: System::Delegate) { self.instance2::<"RemoveEventHandler", System::Object, System::Delegate, ()>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::EventInfo, a2: System::Reflection::EventInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::EventInfo, System::Reflection::EventInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::EventInfo, a2: System::Reflection::EventInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::EventInfo, System::Reflection::EventInfo, bool>(a1, a2) }
}
pub type ExceptionHandlingClause =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ExceptionHandlingClause">;
use super::super::*;
impl From<ExceptionHandlingClause> for System::Object {
 fn from(v:ExceptionHandlingClause)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExceptionHandlingClause>(v)
}} 
impl ExceptionHandlingClause {
    pub fn get_try_offset(self) -> i32 { self.virt0::<"get_TryOffset", i32>() }
    pub fn get_try_length(self) -> i32 { self.virt0::<"get_TryLength", i32>() }
    pub fn get_handler_offset(self) -> i32 { self.virt0::<"get_HandlerOffset", i32>() }
    pub fn get_handler_length(self) -> i32 { self.virt0::<"get_HandlerLength", i32>() }
    pub fn get_filter_offset(self) -> i32 { self.virt0::<"get_FilterOffset", i32>() }
    pub fn get_catch_type(self) -> System::Type { self.virt0::<"get_CatchType", System::Type>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ICustomAttributeProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ICustomAttributeProvider">;
use super::super::*;
pub type IntrospectionExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.IntrospectionExtensions">;
use super::super::*;
impl From<IntrospectionExtensions> for System::Object {
 fn from(v:IntrospectionExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,IntrospectionExtensions>(v)
}} 
impl IntrospectionExtensions {
    pub fn get_type_info(a1: System::Type) -> System::Reflection::TypeInfo { Self::static1::<"GetTypeInfo", System::Type, System::Reflection::TypeInfo>(a1) }
}
pub type InvalidFilterCriteriaException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.InvalidFilterCriteriaException">;
use super::super::*;
impl From<InvalidFilterCriteriaException> for System::ApplicationException {
 fn from(v:InvalidFilterCriteriaException)->System::ApplicationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ApplicationException,InvalidFilterCriteriaException>(v)
}} 
impl InvalidFilterCriteriaException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IReflect =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.IReflect">;
use super::super::*;
impl IReflect {
    pub fn get_underlying_system_type(self) -> System::Type { self.virt0::<"get_UnderlyingSystemType", System::Type>() }
}
pub type IReflectableType =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.IReflectableType">;
use super::super::*;
impl IReflectableType {
    pub fn get_type_info(self) -> System::Reflection::TypeInfo { self.virt0::<"GetTypeInfo", System::Reflection::TypeInfo>() }
}
pub type LocalVariableInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.LocalVariableInfo">;
use super::super::*;
impl From<LocalVariableInfo> for System::Object {
 fn from(v:LocalVariableInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,LocalVariableInfo>(v)
}} 
impl LocalVariableInfo {
    pub fn get_local_type(self) -> System::Type { self.virt0::<"get_LocalType", System::Type>() }
    pub fn get_local_index(self) -> i32 { self.virt0::<"get_LocalIndex", i32>() }
    pub fn get_is_pinned(self) -> bool { self.virt0::<"get_IsPinned", bool>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ManifestResourceInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ManifestResourceInfo">;
use super::super::*;
impl From<ManifestResourceInfo> for System::Object {
 fn from(v:ManifestResourceInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ManifestResourceInfo>(v)
}} 
impl ManifestResourceInfo {
    pub fn get_referenced_assembly(self) -> System::Reflection::Assembly { self.virt0::<"get_ReferencedAssembly", System::Reflection::Assembly>() }
    pub fn get_file_name(self) -> System::String { self.virt0::<"get_FileName", System::String>() }
}
pub type MemberFilter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MemberFilter">;
use super::super::*;
impl From<MemberFilter> for System::MulticastDelegate {
 fn from(v:MemberFilter)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,MemberFilter>(v)
}} 
impl MemberFilter {
    pub fn invoke(self, a1: System::Reflection::MemberInfo, a2: System::Object) -> bool { self.instance2::<"Invoke", System::Reflection::MemberInfo, System::Object, bool>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) -> bool { self.instance1::<"EndInvoke", System::IAsyncResult, bool>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type MethodBody =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MethodBody">;
use super::super::*;
impl From<MethodBody> for System::Object {
 fn from(v:MethodBody)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MethodBody>(v)
}} 
impl MethodBody {
    pub fn get_local_signature_metadata_token(self) -> i32 { self.virt0::<"get_LocalSignatureMetadataToken", i32>() }
    pub fn get_max_stack_size(self) -> i32 { self.virt0::<"get_MaxStackSize", i32>() }
    pub fn get_init_locals(self) -> bool { self.virt0::<"get_InitLocals", bool>() }
}
pub type MethodInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.MethodInfo">;
use super::super::*;
impl From<MethodInfo> for System::Reflection::MethodBase {
 fn from(v:MethodInfo)->System::Reflection::MethodBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MethodBase,MethodInfo>(v)
}} 
impl MethodInfo {
    pub fn get_return_parameter(self) -> System::Reflection::ParameterInfo { self.virt0::<"get_ReturnParameter", System::Reflection::ParameterInfo>() }
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_generic_method_definition(self) -> System::Reflection::MethodInfo { self.virt0::<"GetGenericMethodDefinition", System::Reflection::MethodInfo>() }
    pub fn get_base_definition(self) -> System::Reflection::MethodInfo { self.virt0::<"GetBaseDefinition", System::Reflection::MethodInfo>() }
    pub fn get_return_type_custom_attributes(self) -> System::Reflection::ICustomAttributeProvider { self.virt0::<"get_ReturnTypeCustomAttributes", System::Reflection::ICustomAttributeProvider>() }
    pub fn create_delegate(self, a1: System::Type) -> System::Delegate { self.instance1::<"CreateDelegate", System::Type, System::Delegate>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::MethodInfo, a2: System::Reflection::MethodInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::MethodInfo, System::Reflection::MethodInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::MethodInfo, a2: System::Reflection::MethodInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::MethodInfo, System::Reflection::MethodInfo, bool>(a1, a2) }
}
pub type Missing =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Missing">;
use super::super::*;
impl From<Missing> for System::Object {
 fn from(v:Missing)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Missing>(v)
}} 
pub type Module =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Module">;
use super::super::*;
impl From<Module> for System::Object {
 fn from(v:Module)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Module>(v)
}} 
impl Module {
    pub fn get_assembly(self) -> System::Reflection::Assembly { self.virt0::<"get_Assembly", System::Reflection::Assembly>() }
    pub fn get_fully_qualified_name(self) -> System::String { self.virt0::<"get_FullyQualifiedName", System::String>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_mdstream_version(self) -> i32 { self.virt0::<"get_MDStreamVersion", i32>() }
    pub fn get_scope_name(self) -> System::String { self.virt0::<"get_ScopeName", System::String>() }
    pub fn is_resource(self) -> bool { self.virt0::<"IsResource", bool>() }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn get_method(self, a1: System::String) -> System::Reflection::MethodInfo { self.instance1::<"GetMethod", System::String, System::Reflection::MethodInfo>(a1) }
    pub fn get_field(self, a1: System::String) -> System::Reflection::FieldInfo { self.instance1::<"GetField", System::String, System::Reflection::FieldInfo>(a1) }
    pub fn get_type(self, a1: System::String) -> System::Type { self.instance1::<"GetType", System::String, System::Type>(a1) }
    pub fn get_metadata_token(self) -> i32 { self.virt0::<"get_MetadataToken", i32>() }
    pub fn resolve_field(self, a1: i32) -> System::Reflection::FieldInfo { self.instance1::<"ResolveField", i32, System::Reflection::FieldInfo>(a1) }
    pub fn resolve_member(self, a1: i32) -> System::Reflection::MemberInfo { self.instance1::<"ResolveMember", i32, System::Reflection::MemberInfo>(a1) }
    pub fn resolve_method(self, a1: i32) -> System::Reflection::MethodBase { self.instance1::<"ResolveMethod", i32, System::Reflection::MethodBase>(a1) }
    pub fn resolve_string(self, a1: i32) -> System::String { self.instance1::<"ResolveString", i32, System::String>(a1) }
    pub fn resolve_type(self, a1: i32) -> System::Type { self.instance1::<"ResolveType", i32, System::Type>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::Module, a2: System::Reflection::Module) -> bool { Self::static2::<"op_Equality", System::Reflection::Module, System::Reflection::Module, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::Module, a2: System::Reflection::Module) -> bool { Self::static2::<"op_Inequality", System::Reflection::Module, System::Reflection::Module, bool>(a1, a2) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ModuleResolveEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ModuleResolveEventHandler">;
use super::super::*;
impl From<ModuleResolveEventHandler> for System::MulticastDelegate {
 fn from(v:ModuleResolveEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ModuleResolveEventHandler>(v)
}} 
impl ModuleResolveEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::ResolveEventArgs) -> System::Reflection::Module { self.instance2::<"Invoke", System::Object, System::ResolveEventArgs, System::Reflection::Module>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) -> System::Reflection::Module { self.instance1::<"EndInvoke", System::IAsyncResult, System::Reflection::Module>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type NullabilityInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.NullabilityInfo">;
use super::super::*;
impl From<NullabilityInfo> for System::Object {
 fn from(v:NullabilityInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NullabilityInfo>(v)
}} 
impl NullabilityInfo {
    pub fn get_type(self) -> System::Type { self.instance0::<"get_Type", System::Type>() }
    pub fn get_element_type(self) -> System::Reflection::NullabilityInfo { self.instance0::<"get_ElementType", System::Reflection::NullabilityInfo>() }
}
pub type NullabilityInfoContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.NullabilityInfoContext">;
use super::super::*;
impl From<NullabilityInfoContext> for System::Object {
 fn from(v:NullabilityInfoContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NullabilityInfoContext>(v)
}} 
impl NullabilityInfoContext {
    pub fn create(self, a1: System::Reflection::ParameterInfo) -> System::Reflection::NullabilityInfo { self.instance1::<"Create", System::Reflection::ParameterInfo, System::Reflection::NullabilityInfo>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ObfuscateAssemblyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ObfuscateAssemblyAttribute">;
use super::super::*;
impl From<ObfuscateAssemblyAttribute> for System::Attribute {
 fn from(v:ObfuscateAssemblyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ObfuscateAssemblyAttribute>(v)
}} 
impl ObfuscateAssemblyAttribute {
    pub fn get_assembly_is_private(self) -> bool { self.instance0::<"get_AssemblyIsPrivate", bool>() }
    pub fn get_strip_after_obfuscation(self) -> bool { self.instance0::<"get_StripAfterObfuscation", bool>() }
    pub fn set_strip_after_obfuscation(self, a1: bool) { self.instance1::<"set_StripAfterObfuscation", bool, ()>(a1) }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ObfuscationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ObfuscationAttribute">;
use super::super::*;
impl From<ObfuscationAttribute> for System::Attribute {
 fn from(v:ObfuscationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ObfuscationAttribute>(v)
}} 
impl ObfuscationAttribute {
    pub fn get_strip_after_obfuscation(self) -> bool { self.instance0::<"get_StripAfterObfuscation", bool>() }
    pub fn set_strip_after_obfuscation(self, a1: bool) { self.instance1::<"set_StripAfterObfuscation", bool, ()>(a1) }
    pub fn get_exclude(self) -> bool { self.instance0::<"get_Exclude", bool>() }
    pub fn set_exclude(self, a1: bool) { self.instance1::<"set_Exclude", bool, ()>(a1) }
    pub fn get_apply_to_members(self) -> bool { self.instance0::<"get_ApplyToMembers", bool>() }
    pub fn set_apply_to_members(self, a1: bool) { self.instance1::<"set_ApplyToMembers", bool, ()>(a1) }
    pub fn get_feature(self) -> System::String { self.instance0::<"get_Feature", System::String>() }
    pub fn set_feature(self, a1: System::String) { self.instance1::<"set_Feature", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ParameterInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ParameterInfo">;
use super::super::*;
impl From<ParameterInfo> for System::Object {
 fn from(v:ParameterInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ParameterInfo>(v)
}} 
impl ParameterInfo {
    pub fn get_member(self) -> System::Reflection::MemberInfo { self.virt0::<"get_Member", System::Reflection::MemberInfo>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_parameter_type(self) -> System::Type { self.virt0::<"get_ParameterType", System::Type>() }
    pub fn get_position(self) -> i32 { self.virt0::<"get_Position", i32>() }
    pub fn get_is_in(self) -> bool { self.instance0::<"get_IsIn", bool>() }
    pub fn get_is_lcid(self) -> bool { self.instance0::<"get_IsLcid", bool>() }
    pub fn get_is_optional(self) -> bool { self.instance0::<"get_IsOptional", bool>() }
    pub fn get_is_out(self) -> bool { self.instance0::<"get_IsOut", bool>() }
    pub fn get_is_retval(self) -> bool { self.instance0::<"get_IsRetval", bool>() }
    pub fn get_default_value(self) -> System::Object { self.virt0::<"get_DefaultValue", System::Object>() }
    pub fn get_raw_default_value(self) -> System::Object { self.virt0::<"get_RawDefaultValue", System::Object>() }
    pub fn get_has_default_value(self) -> bool { self.virt0::<"get_HasDefaultValue", bool>() }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn get_modified_parameter_type(self) -> System::Type { self.virt0::<"GetModifiedParameterType", System::Type>() }
    pub fn get_metadata_token(self) -> i32 { self.virt0::<"get_MetadataToken", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type Pointer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.Pointer">;
use super::super::*;
impl From<Pointer> for System::Object {
 fn from(v:Pointer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Pointer>(v)
}} 
impl Pointer {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
}
pub type PropertyInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.PropertyInfo">;
use super::super::*;
impl From<PropertyInfo> for System::Reflection::MemberInfo {
 fn from(v:PropertyInfo)->System::Reflection::MemberInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MemberInfo,PropertyInfo>(v)
}} 
impl PropertyInfo {
    pub fn get_property_type(self) -> System::Type { self.virt0::<"get_PropertyType", System::Type>() }
    pub fn get_is_special_name(self) -> bool { self.instance0::<"get_IsSpecialName", bool>() }
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn get_get_method(self) -> System::Reflection::MethodInfo { self.virt0::<"get_GetMethod", System::Reflection::MethodInfo>() }
    pub fn get_set_method(self) -> System::Reflection::MethodInfo { self.virt0::<"get_SetMethod", System::Reflection::MethodInfo>() }
    pub fn get_modified_property_type(self) -> System::Type { self.virt0::<"GetModifiedPropertyType", System::Type>() }
    pub fn get_value(self, a1: System::Object) -> System::Object { self.instance1::<"GetValue", System::Object, System::Object>(a1) }
    pub fn get_constant_value(self) -> System::Object { self.virt0::<"GetConstantValue", System::Object>() }
    pub fn get_raw_constant_value(self) -> System::Object { self.virt0::<"GetRawConstantValue", System::Object>() }
    pub fn set_value(self, a1: System::Object, a2: System::Object) { self.instance2::<"SetValue", System::Object, System::Object, ()>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Reflection::PropertyInfo, a2: System::Reflection::PropertyInfo) -> bool { Self::static2::<"op_Equality", System::Reflection::PropertyInfo, System::Reflection::PropertyInfo, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Reflection::PropertyInfo, a2: System::Reflection::PropertyInfo) -> bool { Self::static2::<"op_Inequality", System::Reflection::PropertyInfo, System::Reflection::PropertyInfo, bool>(a1, a2) }
}
pub type ReflectionContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ReflectionContext">;
use super::super::*;
impl From<ReflectionContext> for System::Object {
 fn from(v:ReflectionContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ReflectionContext>(v)
}} 
impl ReflectionContext {
    pub fn get_type_for_object(self, a1: System::Object) -> System::Reflection::TypeInfo { self.instance1::<"GetTypeForObject", System::Object, System::Reflection::TypeInfo>(a1) }
}
pub type ReflectionTypeLoadException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.ReflectionTypeLoadException">;
use super::super::*;
impl From<ReflectionTypeLoadException> for System::SystemException {
 fn from(v:ReflectionTypeLoadException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ReflectionTypeLoadException>(v)
}} 
impl ReflectionTypeLoadException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type RuntimeReflectionExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.RuntimeReflectionExtensions">;
use super::super::*;
impl From<RuntimeReflectionExtensions> for System::Object {
 fn from(v:RuntimeReflectionExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RuntimeReflectionExtensions>(v)
}} 
impl RuntimeReflectionExtensions {
    pub fn get_runtime_field(a1: System::Type, a2: System::String) -> System::Reflection::FieldInfo { Self::static2::<"GetRuntimeField", System::Type, System::String, System::Reflection::FieldInfo>(a1, a2) }
    pub fn get_runtime_property(a1: System::Type, a2: System::String) -> System::Reflection::PropertyInfo { Self::static2::<"GetRuntimeProperty", System::Type, System::String, System::Reflection::PropertyInfo>(a1, a2) }
    pub fn get_runtime_event(a1: System::Type, a2: System::String) -> System::Reflection::EventInfo { Self::static2::<"GetRuntimeEvent", System::Type, System::String, System::Reflection::EventInfo>(a1, a2) }
    pub fn get_runtime_base_definition(a1: System::Reflection::MethodInfo) -> System::Reflection::MethodInfo { Self::static1::<"GetRuntimeBaseDefinition", System::Reflection::MethodInfo, System::Reflection::MethodInfo>(a1) }
    pub fn get_method_info(a1: System::Delegate) -> System::Reflection::MethodInfo { Self::static1::<"GetMethodInfo", System::Delegate, System::Reflection::MethodInfo>(a1) }
}
pub type StrongNameKeyPair =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.StrongNameKeyPair">;
use super::super::*;
impl From<StrongNameKeyPair> for System::Object {
 fn from(v:StrongNameKeyPair)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StrongNameKeyPair>(v)
}} 
impl StrongNameKeyPair {
    pub fn new(a1: System::IO::FileStream) -> Self { Self::ctor1(a1) }
}
pub type TargetException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TargetException">;
use super::super::*;
impl From<TargetException> for System::ApplicationException {
 fn from(v:TargetException)->System::ApplicationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ApplicationException,TargetException>(v)
}} 
impl TargetException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TargetInvocationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TargetInvocationException">;
use super::super::*;
impl From<TargetInvocationException> for System::ApplicationException {
 fn from(v:TargetInvocationException)->System::ApplicationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ApplicationException,TargetInvocationException>(v)
}} 
impl TargetInvocationException {
    pub fn new(a1: System::Exception) -> Self { Self::ctor1(a1) }
}
pub type TargetParameterCountException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TargetParameterCountException">;
use super::super::*;
impl From<TargetParameterCountException> for System::ApplicationException {
 fn from(v:TargetParameterCountException)->System::ApplicationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ApplicationException,TargetParameterCountException>(v)
}} 
impl TargetParameterCountException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TypeDelegator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TypeDelegator">;
use super::super::*;
impl From<TypeDelegator> for System::Reflection::TypeInfo {
 fn from(v:TypeDelegator)->System::Reflection::TypeInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::TypeInfo,TypeDelegator>(v)
}} 
impl TypeDelegator {
    pub fn is_assignable_from(self, a1: System::Reflection::TypeInfo) -> bool { self.instance1::<"IsAssignableFrom", System::Reflection::TypeInfo, bool>(a1) }
    pub fn get_metadata_token(self) -> i32 { self.virt0::<"get_MetadataToken", i32>() }
    pub fn get_module(self) -> System::Reflection::Module { self.virt0::<"get_Module", System::Reflection::Module>() }
    pub fn get_assembly(self) -> System::Reflection::Assembly { self.virt0::<"get_Assembly", System::Reflection::Assembly>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_full_name(self) -> System::String { self.virt0::<"get_FullName", System::String>() }
    pub fn get_namespace(self) -> System::String { self.virt0::<"get_Namespace", System::String>() }
    pub fn get_assembly_qualified_name(self) -> System::String { self.virt0::<"get_AssemblyQualifiedName", System::String>() }
    pub fn get_base_type(self) -> System::Type { self.virt0::<"get_BaseType", System::Type>() }
    pub fn get_function_pointer_return_type(self) -> System::Type { self.virt0::<"GetFunctionPointerReturnType", System::Type>() }
    pub fn get_interface(self, a1: System::String, a2: bool) -> System::Type { self.instance2::<"GetInterface", System::String, bool, System::Type>(a1, a2) }
    pub fn get_member_with_same_metadata_definition_as(self, a1: System::Reflection::MemberInfo) -> System::Reflection::MemberInfo { self.instance1::<"GetMemberWithSameMetadataDefinitionAs", System::Reflection::MemberInfo, System::Reflection::MemberInfo>(a1) }
    pub fn get_is_type_definition(self) -> bool { self.virt0::<"get_IsTypeDefinition", bool>() }
    pub fn get_is_szarray(self) -> bool { self.virt0::<"get_IsSZArray", bool>() }
    pub fn get_is_variable_bound_array(self) -> bool { self.virt0::<"get_IsVariableBoundArray", bool>() }
    pub fn get_is_generic_type_parameter(self) -> bool { self.virt0::<"get_IsGenericTypeParameter", bool>() }
    pub fn get_is_generic_method_parameter(self) -> bool { self.virt0::<"get_IsGenericMethodParameter", bool>() }
    pub fn get_is_by_ref_like(self) -> bool { self.virt0::<"get_IsByRefLike", bool>() }
    pub fn get_is_constructed_generic_type(self) -> bool { self.virt0::<"get_IsConstructedGenericType", bool>() }
    pub fn get_is_collectible(self) -> bool { self.virt0::<"get_IsCollectible", bool>() }
    pub fn get_is_function_pointer(self) -> bool { self.virt0::<"get_IsFunctionPointer", bool>() }
    pub fn get_is_unmanaged_function_pointer(self) -> bool { self.virt0::<"get_IsUnmanagedFunctionPointer", bool>() }
    pub fn get_element_type(self) -> System::Type { self.virt0::<"GetElementType", System::Type>() }
    pub fn get_underlying_system_type(self) -> System::Type { self.virt0::<"get_UnderlyingSystemType", System::Type>() }
    pub fn is_defined(self, a1: System::Type, a2: bool) -> bool { self.instance2::<"IsDefined", System::Type, bool, bool>(a1, a2) }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type TypeFilter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TypeFilter">;
use super::super::*;
impl From<TypeFilter> for System::MulticastDelegate {
 fn from(v:TypeFilter)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,TypeFilter>(v)
}} 
impl TypeFilter {
    pub fn invoke(self, a1: System::Type, a2: System::Object) -> bool { self.instance2::<"Invoke", System::Type, System::Object, bool>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) -> bool { self.instance1::<"EndInvoke", System::IAsyncResult, bool>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type TypeInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Reflection.TypeInfo">;
use super::super::*;
impl From<TypeInfo> for System::Type {
 fn from(v:TypeInfo)->System::Type{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Type,TypeInfo>(v)
}} 
impl TypeInfo {
    pub fn as_type(self) -> System::Type { self.virt0::<"AsType", System::Type>() }
    pub fn get_declared_event(self, a1: System::String) -> System::Reflection::EventInfo { self.instance1::<"GetDeclaredEvent", System::String, System::Reflection::EventInfo>(a1) }
    pub fn get_declared_field(self, a1: System::String) -> System::Reflection::FieldInfo { self.instance1::<"GetDeclaredField", System::String, System::Reflection::FieldInfo>(a1) }
    pub fn get_declared_method(self, a1: System::String) -> System::Reflection::MethodInfo { self.instance1::<"GetDeclaredMethod", System::String, System::Reflection::MethodInfo>(a1) }
    pub fn get_declared_nested_type(self, a1: System::String) -> System::Reflection::TypeInfo { self.instance1::<"GetDeclaredNestedType", System::String, System::Reflection::TypeInfo>(a1) }
    pub fn get_declared_property(self, a1: System::String) -> System::Reflection::PropertyInfo { self.instance1::<"GetDeclaredProperty", System::String, System::Reflection::PropertyInfo>(a1) }
    pub fn is_assignable_from(self, a1: System::Reflection::TypeInfo) -> bool { self.instance1::<"IsAssignableFrom", System::Reflection::TypeInfo, bool>(a1) }
}
pub type ICustomTypeProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Reflection.ICustomTypeProvider">;
use super::super::*;
impl ICustomTypeProvider {
    pub fn get_custom_type(self) -> System::Type { self.virt0::<"GetCustomType", System::Type>() }
}
}
pub mod IO{
pub mod Enumeration{
pub type FileSystemName =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.Enumeration.FileSystemName">;
use super::super::super::*;
impl From<FileSystemName> for System::Object {
 fn from(v:FileSystemName)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,FileSystemName>(v)
}} 
impl FileSystemName {
    pub fn translate_win32_expression(a1: System::String) -> System::String { Self::static1::<"TranslateWin32Expression", System::String, System::String>(a1) }
}
}
pub type FileLoadException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileLoadException">;
use super::super::*;
impl From<FileLoadException> for System::IO::IOException {
 fn from(v:FileLoadException)->System::IO::IOException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::IOException,FileLoadException>(v)
}} 
impl FileLoadException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_file_name(self) -> System::String { self.instance0::<"get_FileName", System::String>() }
    pub fn get_fusion_log(self) -> System::String { self.instance0::<"get_FusionLog", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type FileNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileNotFoundException">;
use super::super::*;
impl From<FileNotFoundException> for System::IO::IOException {
 fn from(v:FileNotFoundException)->System::IO::IOException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::IOException,FileNotFoundException>(v)
}} 
impl FileNotFoundException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_file_name(self) -> System::String { self.instance0::<"get_FileName", System::String>() }
    pub fn get_fusion_log(self) -> System::String { self.instance0::<"get_FusionLog", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type BinaryReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.BinaryReader">;
use super::super::*;
impl From<BinaryReader> for System::Object {
 fn from(v:BinaryReader)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BinaryReader>(v)
}} 
impl BinaryReader {
    pub fn get_base_stream(self) -> System::IO::Stream { self.virt0::<"get_BaseStream", System::IO::Stream>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn peek_char(self) -> i32 { self.virt0::<"PeekChar", i32>() }
    pub fn read(self) -> i32 { self.virt0::<"Read", i32>() }
    pub fn read_byte(self) -> u8 { self.virt0::<"ReadByte", u8>() }
    pub fn read_sbyte(self) -> i8 { self.virt0::<"ReadSByte", i8>() }
    pub fn read_boolean(self) -> bool { self.virt0::<"ReadBoolean", bool>() }
    pub fn read_int16(self) -> i16 { self.virt0::<"ReadInt16", i16>() }
    pub fn read_uint16(self) -> u16 { self.virt0::<"ReadUInt16", u16>() }
    pub fn read_int32(self) -> i32 { self.virt0::<"ReadInt32", i32>() }
    pub fn read_uint32(self) -> u32 { self.virt0::<"ReadUInt32", u32>() }
    pub fn read_int64(self) -> i64 { self.virt0::<"ReadInt64", i64>() }
    pub fn read_uint64(self) -> u64 { self.virt0::<"ReadUInt64", u64>() }
    pub fn read_single(self) -> f32 { self.virt0::<"ReadSingle", f32>() }
    pub fn read_double(self) -> f64 { self.virt0::<"ReadDouble", f64>() }
    pub fn read_string(self) -> System::String { self.virt0::<"ReadString", System::String>() }
    pub fn read7_bit_encoded_int(self) -> i32 { self.instance0::<"Read7BitEncodedInt", i32>() }
    pub fn read7_bit_encoded_int64(self) -> i64 { self.instance0::<"Read7BitEncodedInt64", i64>() }
    pub fn new(a1: System::IO::Stream) -> Self { Self::ctor1(a1) }
}
pub type BinaryWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.BinaryWriter">;
use super::super::*;
impl From<BinaryWriter> for System::Object {
 fn from(v:BinaryWriter)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BinaryWriter>(v)
}} 
impl BinaryWriter {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn get_base_stream(self) -> System::IO::Stream { self.virt0::<"get_BaseStream", System::IO::Stream>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn write(self, a1: bool) { self.instance1::<"Write", bool, ()>(a1) }
    pub fn write7_bit_encoded_int(self, a1: i32) { self.instance1::<"Write7BitEncodedInt", i32, ()>(a1) }
    pub fn write7_bit_encoded_int64(self, a1: i64) { self.instance1::<"Write7BitEncodedInt64", i64, ()>(a1) }
    pub fn new(a1: System::IO::Stream) -> Self { Self::ctor1(a1) }
}
pub type BufferedStream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.BufferedStream">;
use super::super::*;
impl From<BufferedStream> for System::IO::Stream {
 fn from(v:BufferedStream)->System::IO::Stream{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::Stream,BufferedStream>(v)
}} 
impl BufferedStream {
    pub fn get_underlying_stream(self) -> System::IO::Stream { self.instance0::<"get_UnderlyingStream", System::IO::Stream>() }
    pub fn get_buffer_size(self) -> i32 { self.instance0::<"get_BufferSize", i32>() }
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn get_can_seek(self) -> bool { self.virt0::<"get_CanSeek", bool>() }
    pub fn get_length(self) -> i64 { self.virt0::<"get_Length", i64>() }
    pub fn get_position(self) -> i64 { self.virt0::<"get_Position", i64>() }
    pub fn set_position(self, a1: i64) { self.instance1::<"set_Position", i64, ()>(a1) }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn end_read(self, a1: System::IAsyncResult) -> i32 { self.instance1::<"EndRead", System::IAsyncResult, i32>(a1) }
    pub fn read_byte(self) -> i32 { self.virt0::<"ReadByte", i32>() }
    pub fn end_write(self, a1: System::IAsyncResult) { self.instance1::<"EndWrite", System::IAsyncResult, ()>(a1) }
    pub fn write_byte(self, a1: u8) { self.instance1::<"WriteByte", u8, ()>(a1) }
    pub fn set_length(self, a1: i64) { self.instance1::<"SetLength", i64, ()>(a1) }
    pub fn copy_to(self, a1: System::IO::Stream, a2: i32) { self.instance2::<"CopyTo", System::IO::Stream, i32, ()>(a1, a2) }
    pub fn new(a1: System::IO::Stream) -> Self { Self::ctor1(a1) }
}
pub type Directory =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.Directory">;
use super::super::*;
impl From<Directory> for System::Object {
 fn from(v:Directory)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Directory>(v)
}} 
impl Directory {
    pub fn get_parent(a1: System::String) -> System::IO::DirectoryInfo { Self::static1::<"GetParent", System::String, System::IO::DirectoryInfo>(a1) }
    pub fn create_directory(a1: System::String) -> System::IO::DirectoryInfo { Self::static1::<"CreateDirectory", System::String, System::IO::DirectoryInfo>(a1) }
    pub fn create_temp_subdirectory(a1: System::String) -> System::IO::DirectoryInfo { Self::static1::<"CreateTempSubdirectory", System::String, System::IO::DirectoryInfo>(a1) }
    pub fn exists(a1: System::String) -> bool { Self::static1::<"Exists", System::String, bool>(a1) }
    pub fn get_directory_root(a1: System::String) -> System::String { Self::static1::<"GetDirectoryRoot", System::String, System::String>(a1) }
    pub fn get_current_directory() -> System::String { Self::static0::<"GetCurrentDirectory", System::String>() }
    pub fn set_current_directory(a1: System::String) { Self::static1::<"SetCurrentDirectory", System::String, ()>(a1) }
    pub fn r#move(a1: System::String, a2: System::String) { Self::static2::<"Move", System::String, System::String, ()>(a1, a2) }
    pub fn delete(a1: System::String) { Self::static1::<"Delete", System::String, ()>(a1) }
    pub fn create_symbolic_link(a1: System::String, a2: System::String) -> System::IO::FileSystemInfo { Self::static2::<"CreateSymbolicLink", System::String, System::String, System::IO::FileSystemInfo>(a1, a2) }
    pub fn resolve_link_target(a1: System::String, a2: bool) -> System::IO::FileSystemInfo { Self::static2::<"ResolveLinkTarget", System::String, bool, System::IO::FileSystemInfo>(a1, a2) }
}
pub type DirectoryInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.DirectoryInfo">;
use super::super::*;
impl From<DirectoryInfo> for System::IO::FileSystemInfo {
 fn from(v:DirectoryInfo)->System::IO::FileSystemInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::FileSystemInfo,DirectoryInfo>(v)
}} 
impl DirectoryInfo {
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_parent(self) -> System::IO::DirectoryInfo { self.instance0::<"get_Parent", System::IO::DirectoryInfo>() }
    pub fn create_subdirectory(self, a1: System::String) -> System::IO::DirectoryInfo { self.instance1::<"CreateSubdirectory", System::String, System::IO::DirectoryInfo>(a1) }
    pub fn create(self) { self.instance0::<"Create", ()>() }
    pub fn get_root(self) -> System::IO::DirectoryInfo { self.instance0::<"get_Root", System::IO::DirectoryInfo>() }
    pub fn move_to(self, a1: System::String) { self.instance1::<"MoveTo", System::String, ()>(a1) }
    pub fn delete(self) { self.virt0::<"Delete", ()>() }
    pub fn get_exists(self) -> bool { self.virt0::<"get_Exists", bool>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type DirectoryNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.DirectoryNotFoundException">;
use super::super::*;
impl From<DirectoryNotFoundException> for System::IO::IOException {
 fn from(v:DirectoryNotFoundException)->System::IO::IOException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::IOException,DirectoryNotFoundException>(v)
}} 
impl DirectoryNotFoundException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EnumerationOptions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.EnumerationOptions">;
use super::super::*;
impl From<EnumerationOptions> for System::Object {
 fn from(v:EnumerationOptions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EnumerationOptions>(v)
}} 
impl EnumerationOptions {
    pub fn get_recurse_subdirectories(self) -> bool { self.instance0::<"get_RecurseSubdirectories", bool>() }
    pub fn set_recurse_subdirectories(self, a1: bool) { self.instance1::<"set_RecurseSubdirectories", bool, ()>(a1) }
    pub fn get_ignore_inaccessible(self) -> bool { self.instance0::<"get_IgnoreInaccessible", bool>() }
    pub fn set_ignore_inaccessible(self, a1: bool) { self.instance1::<"set_IgnoreInaccessible", bool, ()>(a1) }
    pub fn get_buffer_size(self) -> i32 { self.instance0::<"get_BufferSize", i32>() }
    pub fn set_buffer_size(self, a1: i32) { self.instance1::<"set_BufferSize", i32, ()>(a1) }
    pub fn get_max_recursion_depth(self) -> i32 { self.instance0::<"get_MaxRecursionDepth", i32>() }
    pub fn set_max_recursion_depth(self, a1: i32) { self.instance1::<"set_MaxRecursionDepth", i32, ()>(a1) }
    pub fn get_return_special_directories(self) -> bool { self.instance0::<"get_ReturnSpecialDirectories", bool>() }
    pub fn set_return_special_directories(self, a1: bool) { self.instance1::<"set_ReturnSpecialDirectories", bool, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EndOfStreamException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.EndOfStreamException">;
use super::super::*;
impl From<EndOfStreamException> for System::IO::IOException {
 fn from(v:EndOfStreamException)->System::IO::IOException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::IOException,EndOfStreamException>(v)
}} 
impl EndOfStreamException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type File =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.File">;
use super::super::*;
impl From<File> for System::Object {
 fn from(v:File)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,File>(v)
}} 
impl File {
    pub fn open_text(a1: System::String) -> System::IO::StreamReader { Self::static1::<"OpenText", System::String, System::IO::StreamReader>(a1) }
    pub fn create_text(a1: System::String) -> System::IO::StreamWriter { Self::static1::<"CreateText", System::String, System::IO::StreamWriter>(a1) }
    pub fn append_text(a1: System::String) -> System::IO::StreamWriter { Self::static1::<"AppendText", System::String, System::IO::StreamWriter>(a1) }
    pub fn copy(a1: System::String, a2: System::String) { Self::static2::<"Copy", System::String, System::String, ()>(a1, a2) }
    pub fn create(a1: System::String) -> System::IO::FileStream { Self::static1::<"Create", System::String, System::IO::FileStream>(a1) }
    pub fn delete(a1: System::String) { Self::static1::<"Delete", System::String, ()>(a1) }
    pub fn exists(a1: System::String) -> bool { Self::static1::<"Exists", System::String, bool>(a1) }
    pub fn open(a1: System::String, a2: System::IO::FileStreamOptions) -> System::IO::FileStream { Self::static2::<"Open", System::String, System::IO::FileStreamOptions, System::IO::FileStream>(a1, a2) }
    pub fn open_read(a1: System::String) -> System::IO::FileStream { Self::static1::<"OpenRead", System::String, System::IO::FileStream>(a1) }
    pub fn open_write(a1: System::String) -> System::IO::FileStream { Self::static1::<"OpenWrite", System::String, System::IO::FileStream>(a1) }
    pub fn read_all_text(a1: System::String) -> System::String { Self::static1::<"ReadAllText", System::String, System::String>(a1) }
    pub fn write_all_text(a1: System::String, a2: System::String) { Self::static2::<"WriteAllText", System::String, System::String, ()>(a1, a2) }
    pub fn append_all_text(a1: System::String, a2: System::String) { Self::static2::<"AppendAllText", System::String, System::String, ()>(a1, a2) }
    pub fn r#move(a1: System::String, a2: System::String) { Self::static2::<"Move", System::String, System::String, ()>(a1, a2) }
    pub fn encrypt(a1: System::String) { Self::static1::<"Encrypt", System::String, ()>(a1) }
    pub fn decrypt(a1: System::String) { Self::static1::<"Decrypt", System::String, ()>(a1) }
    pub fn create_symbolic_link(a1: System::String, a2: System::String) -> System::IO::FileSystemInfo { Self::static2::<"CreateSymbolicLink", System::String, System::String, System::IO::FileSystemInfo>(a1, a2) }
    pub fn resolve_link_target(a1: System::String, a2: bool) -> System::IO::FileSystemInfo { Self::static2::<"ResolveLinkTarget", System::String, bool, System::IO::FileSystemInfo>(a1, a2) }
}
pub type FileInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileInfo">;
use super::super::*;
impl From<FileInfo> for System::IO::FileSystemInfo {
 fn from(v:FileInfo)->System::IO::FileSystemInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::FileSystemInfo,FileInfo>(v)
}} 
impl FileInfo {
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_length(self) -> i64 { self.instance0::<"get_Length", i64>() }
    pub fn get_directory_name(self) -> System::String { self.instance0::<"get_DirectoryName", System::String>() }
    pub fn get_directory(self) -> System::IO::DirectoryInfo { self.instance0::<"get_Directory", System::IO::DirectoryInfo>() }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn set_is_read_only(self, a1: bool) { self.instance1::<"set_IsReadOnly", bool, ()>(a1) }
    pub fn open(self, a1: System::IO::FileStreamOptions) -> System::IO::FileStream { self.instance1::<"Open", System::IO::FileStreamOptions, System::IO::FileStream>(a1) }
    pub fn open_text(self) -> System::IO::StreamReader { self.instance0::<"OpenText", System::IO::StreamReader>() }
    pub fn create_text(self) -> System::IO::StreamWriter { self.instance0::<"CreateText", System::IO::StreamWriter>() }
    pub fn append_text(self) -> System::IO::StreamWriter { self.instance0::<"AppendText", System::IO::StreamWriter>() }
    pub fn copy_to(self, a1: System::String) -> System::IO::FileInfo { self.instance1::<"CopyTo", System::String, System::IO::FileInfo>(a1) }
    pub fn create(self) -> System::IO::FileStream { self.instance0::<"Create", System::IO::FileStream>() }
    pub fn delete(self) { self.virt0::<"Delete", ()>() }
    pub fn get_exists(self) -> bool { self.virt0::<"get_Exists", bool>() }
    pub fn open_read(self) -> System::IO::FileStream { self.instance0::<"OpenRead", System::IO::FileStream>() }
    pub fn open_write(self) -> System::IO::FileStream { self.instance0::<"OpenWrite", System::IO::FileStream>() }
    pub fn move_to(self, a1: System::String) { self.instance1::<"MoveTo", System::String, ()>(a1) }
    pub fn replace(self, a1: System::String, a2: System::String) -> System::IO::FileInfo { self.instance2::<"Replace", System::String, System::String, System::IO::FileInfo>(a1, a2) }
    pub fn decrypt(self) { self.instance0::<"Decrypt", ()>() }
    pub fn encrypt(self) { self.instance0::<"Encrypt", ()>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type FileStream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileStream">;
use super::super::*;
impl From<FileStream> for System::IO::Stream {
 fn from(v:FileStream)->System::IO::Stream{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::Stream,FileStream>(v)
}} 
impl FileStream {
    pub fn get_handle(self) -> isize { self.virt0::<"get_Handle", isize>() }
    pub fn lock(self, a1: i64, a2: i64) { self.instance2::<"Lock", i64, i64, ()>(a1, a2) }
    pub fn unlock(self, a1: i64, a2: i64) { self.instance2::<"Unlock", i64, i64, ()>(a1, a2) }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn set_length(self, a1: i64) { self.instance1::<"SetLength", i64, ()>(a1) }
    pub fn get_safe_file_handle(self) -> Microsoft::Win32::SafeHandles::SafeFileHandle { self.virt0::<"get_SafeFileHandle", Microsoft::Win32::SafeHandles::SafeFileHandle>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_is_async(self) -> bool { self.virt0::<"get_IsAsync", bool>() }
    pub fn get_length(self) -> i64 { self.virt0::<"get_Length", i64>() }
    pub fn get_position(self) -> i64 { self.virt0::<"get_Position", i64>() }
    pub fn set_position(self, a1: i64) { self.instance1::<"set_Position", i64, ()>(a1) }
    pub fn read_byte(self) -> i32 { self.virt0::<"ReadByte", i32>() }
    pub fn write_byte(self, a1: u8) { self.instance1::<"WriteByte", u8, ()>(a1) }
    pub fn copy_to(self, a1: System::IO::Stream, a2: i32) { self.instance2::<"CopyTo", System::IO::Stream, i32, ()>(a1, a2) }
    pub fn end_read(self, a1: System::IAsyncResult) -> i32 { self.instance1::<"EndRead", System::IAsyncResult, i32>(a1) }
    pub fn end_write(self, a1: System::IAsyncResult) { self.instance1::<"EndWrite", System::IAsyncResult, ()>(a1) }
    pub fn get_can_seek(self) -> bool { self.virt0::<"get_CanSeek", bool>() }
    pub fn new(a1: System::String, a2: System::IO::FileStreamOptions) -> Self { Self::ctor2(a1, a2) }
}
pub type FileStreamOptions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileStreamOptions">;
use super::super::*;
impl From<FileStreamOptions> for System::Object {
 fn from(v:FileStreamOptions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,FileStreamOptions>(v)
}} 
impl FileStreamOptions {
    pub fn get_preallocation_size(self) -> i64 { self.instance0::<"get_PreallocationSize", i64>() }
    pub fn set_preallocation_size(self, a1: i64) { self.instance1::<"set_PreallocationSize", i64, ()>(a1) }
    pub fn get_buffer_size(self) -> i32 { self.instance0::<"get_BufferSize", i32>() }
    pub fn set_buffer_size(self, a1: i32) { self.instance1::<"set_BufferSize", i32, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type FileSystemInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.FileSystemInfo">;
use super::super::*;
impl From<FileSystemInfo> for System::MarshalByRefObject {
 fn from(v:FileSystemInfo)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,FileSystemInfo>(v)
}} 
impl FileSystemInfo {
    pub fn get_full_name(self) -> System::String { self.virt0::<"get_FullName", System::String>() }
    pub fn get_extension(self) -> System::String { self.instance0::<"get_Extension", System::String>() }
    pub fn get_name(self) -> System::String { self.virt0::<"get_Name", System::String>() }
    pub fn get_exists(self) -> bool { self.virt0::<"get_Exists", bool>() }
    pub fn delete(self) { self.virt0::<"Delete", ()>() }
    pub fn get_link_target(self) -> System::String { self.instance0::<"get_LinkTarget", System::String>() }
    pub fn create_as_symbolic_link(self, a1: System::String) { self.instance1::<"CreateAsSymbolicLink", System::String, ()>(a1) }
    pub fn resolve_link_target(self, a1: bool) -> System::IO::FileSystemInfo { self.instance1::<"ResolveLinkTarget", bool, System::IO::FileSystemInfo>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn refresh(self) { self.instance0::<"Refresh", ()>() }
}
pub type InvalidDataException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.InvalidDataException">;
use super::super::*;
impl From<InvalidDataException> for System::SystemException {
 fn from(v:InvalidDataException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidDataException>(v)
}} 
impl InvalidDataException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IOException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.IOException">;
use super::super::*;
impl From<IOException> for System::SystemException {
 fn from(v:IOException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,IOException>(v)
}} 
impl IOException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MemoryStream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.MemoryStream">;
use super::super::*;
impl From<MemoryStream> for System::IO::Stream {
 fn from(v:MemoryStream)->System::IO::Stream{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::Stream,MemoryStream>(v)
}} 
impl MemoryStream {
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_seek(self) -> bool { self.virt0::<"get_CanSeek", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn get_capacity(self) -> i32 { self.virt0::<"get_Capacity", i32>() }
    pub fn set_capacity(self, a1: i32) { self.instance1::<"set_Capacity", i32, ()>(a1) }
    pub fn get_length(self) -> i64 { self.virt0::<"get_Length", i64>() }
    pub fn get_position(self) -> i64 { self.virt0::<"get_Position", i64>() }
    pub fn set_position(self, a1: i64) { self.instance1::<"set_Position", i64, ()>(a1) }
    pub fn read_byte(self) -> i32 { self.virt0::<"ReadByte", i32>() }
    pub fn copy_to(self, a1: System::IO::Stream, a2: i32) { self.instance2::<"CopyTo", System::IO::Stream, i32, ()>(a1, a2) }
    pub fn set_length(self, a1: i64) { self.instance1::<"SetLength", i64, ()>(a1) }
    pub fn write_byte(self, a1: u8) { self.instance1::<"WriteByte", u8, ()>(a1) }
    pub fn write_to(self, a1: System::IO::Stream) { self.instance1::<"WriteTo", System::IO::Stream, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Path =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.Path">;
use super::super::*;
impl From<Path> for System::Object {
 fn from(v:Path)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Path>(v)
}} 
impl Path {
    pub fn change_extension(a1: System::String, a2: System::String) -> System::String { Self::static2::<"ChangeExtension", System::String, System::String, System::String>(a1, a2) }
    pub fn exists(a1: System::String) -> bool { Self::static1::<"Exists", System::String, bool>(a1) }
    pub fn get_directory_name(a1: System::String) -> System::String { Self::static1::<"GetDirectoryName", System::String, System::String>(a1) }
    pub fn get_extension(a1: System::String) -> System::String { Self::static1::<"GetExtension", System::String, System::String>(a1) }
    pub fn get_file_name(a1: System::String) -> System::String { Self::static1::<"GetFileName", System::String, System::String>(a1) }
    pub fn get_file_name_without_extension(a1: System::String) -> System::String { Self::static1::<"GetFileNameWithoutExtension", System::String, System::String>(a1) }
    pub fn get_random_file_name() -> System::String { Self::static0::<"GetRandomFileName", System::String>() }
    pub fn is_path_fully_qualified(a1: System::String) -> bool { Self::static1::<"IsPathFullyQualified", System::String, bool>(a1) }
    pub fn has_extension(a1: System::String) -> bool { Self::static1::<"HasExtension", System::String, bool>(a1) }
    pub fn combine(a1: System::String, a2: System::String) -> System::String { Self::static2::<"Combine", System::String, System::String, System::String>(a1, a2) }
    pub fn join(a1: System::String, a2: System::String) -> System::String { Self::static2::<"Join", System::String, System::String, System::String>(a1, a2) }
    pub fn get_relative_path(a1: System::String, a2: System::String) -> System::String { Self::static2::<"GetRelativePath", System::String, System::String, System::String>(a1, a2) }
    pub fn trim_ending_directory_separator(a1: System::String) -> System::String { Self::static1::<"TrimEndingDirectorySeparator", System::String, System::String>(a1) }
    pub fn ends_in_directory_separator(a1: System::String) -> bool { Self::static1::<"EndsInDirectorySeparator", System::String, bool>(a1) }
    pub fn get_full_path(a1: System::String) -> System::String { Self::static1::<"GetFullPath", System::String, System::String>(a1) }
    pub fn get_temp_path() -> System::String { Self::static0::<"GetTempPath", System::String>() }
    pub fn get_temp_file_name() -> System::String { Self::static0::<"GetTempFileName", System::String>() }
    pub fn is_path_rooted(a1: System::String) -> bool { Self::static1::<"IsPathRooted", System::String, bool>(a1) }
    pub fn get_path_root(a1: System::String) -> System::String { Self::static1::<"GetPathRoot", System::String, System::String>(a1) }
}
pub type PathTooLongException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.PathTooLongException">;
use super::super::*;
impl From<PathTooLongException> for System::IO::IOException {
 fn from(v:PathTooLongException)->System::IO::IOException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::IOException,PathTooLongException>(v)
}} 
impl PathTooLongException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type RandomAccess =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.RandomAccess">;
use super::super::*;
impl From<RandomAccess> for System::Object {
 fn from(v:RandomAccess)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,RandomAccess>(v)
}} 
impl RandomAccess {
    pub fn get_length(a1: Microsoft::Win32::SafeHandles::SafeFileHandle) -> i64 { Self::static1::<"GetLength", Microsoft::Win32::SafeHandles::SafeFileHandle, i64>(a1) }
    pub fn set_length(a1: Microsoft::Win32::SafeHandles::SafeFileHandle, a2: i64) { Self::static2::<"SetLength", Microsoft::Win32::SafeHandles::SafeFileHandle, i64, ()>(a1, a2) }
    pub fn flush_to_disk(a1: Microsoft::Win32::SafeHandles::SafeFileHandle) { Self::static1::<"FlushToDisk", Microsoft::Win32::SafeHandles::SafeFileHandle, ()>(a1) }
}
pub type Stream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.Stream">;
use super::super::*;
impl From<Stream> for System::MarshalByRefObject {
 fn from(v:Stream)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,Stream>(v)
}} 
impl Stream {
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn get_can_seek(self) -> bool { self.virt0::<"get_CanSeek", bool>() }
    pub fn get_can_timeout(self) -> bool { self.virt0::<"get_CanTimeout", bool>() }
    pub fn get_length(self) -> i64 { self.virt0::<"get_Length", i64>() }
    pub fn get_position(self) -> i64 { self.virt0::<"get_Position", i64>() }
    pub fn get_read_timeout(self) -> i32 { self.virt0::<"get_ReadTimeout", i32>() }
    pub fn set_read_timeout(self, a1: i32) { self.instance1::<"set_ReadTimeout", i32, ()>(a1) }
    pub fn get_write_timeout(self) -> i32 { self.virt0::<"get_WriteTimeout", i32>() }
    pub fn set_write_timeout(self, a1: i32) { self.instance1::<"set_WriteTimeout", i32, ()>(a1) }
    pub fn copy_to(self, a1: System::IO::Stream) { self.instance1::<"CopyTo", System::IO::Stream, ()>(a1) }
    pub fn copy_to_async(self, a1: System::IO::Stream) -> System::Threading::Tasks::Task { self.instance1::<"CopyToAsync", System::IO::Stream, System::Threading::Tasks::Task>(a1) }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn flush_async(self) -> System::Threading::Tasks::Task { self.instance0::<"FlushAsync", System::Threading::Tasks::Task>() }
    pub fn end_read(self, a1: System::IAsyncResult) -> i32 { self.instance1::<"EndRead", System::IAsyncResult, i32>(a1) }
    pub fn end_write(self, a1: System::IAsyncResult) { self.instance1::<"EndWrite", System::IAsyncResult, ()>(a1) }
    pub fn read_byte(self) -> i32 { self.virt0::<"ReadByte", i32>() }
    pub fn write_byte(self, a1: u8) { self.instance1::<"WriteByte", u8, ()>(a1) }
    pub fn synchronized(a1: System::IO::Stream) -> System::IO::Stream { Self::static1::<"Synchronized", System::IO::Stream, System::IO::Stream>(a1) }
}
pub type StreamReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.StreamReader">;
use super::super::*;
impl From<StreamReader> for System::IO::TextReader {
 fn from(v:StreamReader)->System::IO::TextReader{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::TextReader,StreamReader>(v)
}} 
impl StreamReader {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn get_current_encoding(self) -> System::Text::Encoding { self.virt0::<"get_CurrentEncoding", System::Text::Encoding>() }
    pub fn get_base_stream(self) -> System::IO::Stream { self.virt0::<"get_BaseStream", System::IO::Stream>() }
    pub fn discard_buffered_data(self) { self.instance0::<"DiscardBufferedData", ()>() }
    pub fn get_end_of_stream(self) -> bool { self.instance0::<"get_EndOfStream", bool>() }
    pub fn peek(self) -> i32 { self.virt0::<"Peek", i32>() }
    pub fn read(self) -> i32 { self.virt0::<"Read", i32>() }
    pub fn read_to_end(self) -> System::String { self.virt0::<"ReadToEnd", System::String>() }
    pub fn read_line(self) -> System::String { self.virt0::<"ReadLine", System::String>() }
    pub fn new(a1: System::IO::Stream) -> Self { Self::ctor1(a1) }
}
pub type StreamWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.StreamWriter">;
use super::super::*;
impl From<StreamWriter> for System::IO::TextWriter {
 fn from(v:StreamWriter)->System::IO::TextWriter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::TextWriter,StreamWriter>(v)
}} 
impl StreamWriter {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn get_auto_flush(self) -> bool { self.virt0::<"get_AutoFlush", bool>() }
    pub fn set_auto_flush(self, a1: bool) { self.instance1::<"set_AutoFlush", bool, ()>(a1) }
    pub fn get_base_stream(self) -> System::IO::Stream { self.virt0::<"get_BaseStream", System::IO::Stream>() }
    pub fn get_encoding(self) -> System::Text::Encoding { self.virt0::<"get_Encoding", System::Text::Encoding>() }
    pub fn write(self, a1: System::String) { self.instance1::<"Write", System::String, ()>(a1) }
    pub fn write_line(self, a1: System::String) { self.instance1::<"WriteLine", System::String, ()>(a1) }
    pub fn write_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn write_line_async(self) -> System::Threading::Tasks::Task { self.virt0::<"WriteLineAsync", System::Threading::Tasks::Task>() }
    pub fn flush_async(self) -> System::Threading::Tasks::Task { self.virt0::<"FlushAsync", System::Threading::Tasks::Task>() }
    pub fn new(a1: System::IO::Stream) -> Self { Self::ctor1(a1) }
}
pub type StringReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.StringReader">;
use super::super::*;
impl From<StringReader> for System::IO::TextReader {
 fn from(v:StringReader)->System::IO::TextReader{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::TextReader,StringReader>(v)
}} 
impl StringReader {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn peek(self) -> i32 { self.virt0::<"Peek", i32>() }
    pub fn read(self) -> i32 { self.virt0::<"Read", i32>() }
    pub fn read_to_end(self) -> System::String { self.virt0::<"ReadToEnd", System::String>() }
    pub fn read_line(self) -> System::String { self.virt0::<"ReadLine", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type StringWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.StringWriter">;
use super::super::*;
impl From<StringWriter> for System::IO::TextWriter {
 fn from(v:StringWriter)->System::IO::TextWriter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::TextWriter,StringWriter>(v)
}} 
impl StringWriter {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn get_encoding(self) -> System::Text::Encoding { self.virt0::<"get_Encoding", System::Text::Encoding>() }
    pub fn get_string_builder(self) -> System::Text::StringBuilder { self.virt0::<"GetStringBuilder", System::Text::StringBuilder>() }
    pub fn write(self, a1: System::String) { self.instance1::<"Write", System::String, ()>(a1) }
    pub fn write_line(self, a1: System::Text::StringBuilder) { self.instance1::<"WriteLine", System::Text::StringBuilder, ()>(a1) }
    pub fn write_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn write_line_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteLineAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn flush_async(self) -> System::Threading::Tasks::Task { self.virt0::<"FlushAsync", System::Threading::Tasks::Task>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type TextReader =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.TextReader">;
use super::super::*;
impl From<TextReader> for System::MarshalByRefObject {
 fn from(v:TextReader)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,TextReader>(v)
}} 
impl TextReader {
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn peek(self) -> i32 { self.virt0::<"Peek", i32>() }
    pub fn read(self) -> i32 { self.virt0::<"Read", i32>() }
    pub fn read_to_end(self) -> System::String { self.virt0::<"ReadToEnd", System::String>() }
    pub fn read_line(self) -> System::String { self.virt0::<"ReadLine", System::String>() }
    pub fn synchronized(a1: System::IO::TextReader) -> System::IO::TextReader { Self::static1::<"Synchronized", System::IO::TextReader, System::IO::TextReader>(a1) }
}
pub type TextWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.TextWriter">;
use super::super::*;
impl From<TextWriter> for System::MarshalByRefObject {
 fn from(v:TextWriter)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,TextWriter>(v)
}} 
impl TextWriter {
    pub fn get_format_provider(self) -> System::IFormatProvider { self.virt0::<"get_FormatProvider", System::IFormatProvider>() }
    pub fn close(self) { self.virt0::<"Close", ()>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn get_encoding(self) -> System::Text::Encoding { self.virt0::<"get_Encoding", System::Text::Encoding>() }
    pub fn get_new_line(self) -> System::String { self.virt0::<"get_NewLine", System::String>() }
    pub fn set_new_line(self, a1: System::String) { self.instance1::<"set_NewLine", System::String, ()>(a1) }
    pub fn write(self, a1: bool) { self.instance1::<"Write", bool, ()>(a1) }
    pub fn write_line(self) { self.virt0::<"WriteLine", ()>() }
    pub fn write_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn write_line_async(self, a1: System::String) -> System::Threading::Tasks::Task { self.instance1::<"WriteLineAsync", System::String, System::Threading::Tasks::Task>(a1) }
    pub fn flush_async(self) -> System::Threading::Tasks::Task { self.virt0::<"FlushAsync", System::Threading::Tasks::Task>() }
    pub fn synchronized(a1: System::IO::TextWriter) -> System::IO::TextWriter { Self::static1::<"Synchronized", System::IO::TextWriter, System::IO::TextWriter>(a1) }
}
pub type UnmanagedMemoryAccessor =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.UnmanagedMemoryAccessor">;
use super::super::*;
impl From<UnmanagedMemoryAccessor> for System::Object {
 fn from(v:UnmanagedMemoryAccessor)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,UnmanagedMemoryAccessor>(v)
}} 
impl UnmanagedMemoryAccessor {
    pub fn get_capacity(self) -> i64 { self.instance0::<"get_Capacity", i64>() }
    pub fn get_can_read(self) -> bool { self.instance0::<"get_CanRead", bool>() }
    pub fn get_can_write(self) -> bool { self.instance0::<"get_CanWrite", bool>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn read_boolean(self, a1: i64) -> bool { self.instance1::<"ReadBoolean", i64, bool>(a1) }
    pub fn read_byte(self, a1: i64) -> u8 { self.instance1::<"ReadByte", i64, u8>(a1) }
    pub fn read_int16(self, a1: i64) -> i16 { self.instance1::<"ReadInt16", i64, i16>(a1) }
    pub fn read_int32(self, a1: i64) -> i32 { self.instance1::<"ReadInt32", i64, i32>(a1) }
    pub fn read_int64(self, a1: i64) -> i64 { self.instance1::<"ReadInt64", i64, i64>(a1) }
    pub fn read_single(self, a1: i64) -> f32 { self.instance1::<"ReadSingle", i64, f32>(a1) }
    pub fn read_double(self, a1: i64) -> f64 { self.instance1::<"ReadDouble", i64, f64>(a1) }
    pub fn read_sbyte(self, a1: i64) -> i8 { self.instance1::<"ReadSByte", i64, i8>(a1) }
    pub fn read_uint16(self, a1: i64) -> u16 { self.instance1::<"ReadUInt16", i64, u16>(a1) }
    pub fn read_uint32(self, a1: i64) -> u32 { self.instance1::<"ReadUInt32", i64, u32>(a1) }
    pub fn read_uint64(self, a1: i64) -> u64 { self.instance1::<"ReadUInt64", i64, u64>(a1) }
    pub fn write(self, a1: i64, a2: bool) { self.instance2::<"Write", i64, bool, ()>(a1, a2) }
    pub fn new(a1: System::Runtime::InteropServices::SafeBuffer, a2: i64, a3: i64) -> Self { Self::ctor3(a1, a2, a3) }
}
pub type UnmanagedMemoryStream =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IO.UnmanagedMemoryStream">;
use super::super::*;
impl From<UnmanagedMemoryStream> for System::IO::Stream {
 fn from(v:UnmanagedMemoryStream)->System::IO::Stream{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::IO::Stream,UnmanagedMemoryStream>(v)
}} 
impl UnmanagedMemoryStream {
    pub fn get_can_read(self) -> bool { self.virt0::<"get_CanRead", bool>() }
    pub fn get_can_seek(self) -> bool { self.virt0::<"get_CanSeek", bool>() }
    pub fn get_can_write(self) -> bool { self.virt0::<"get_CanWrite", bool>() }
    pub fn flush(self) { self.virt0::<"Flush", ()>() }
    pub fn get_length(self) -> i64 { self.virt0::<"get_Length", i64>() }
    pub fn get_capacity(self) -> i64 { self.instance0::<"get_Capacity", i64>() }
    pub fn get_position(self) -> i64 { self.virt0::<"get_Position", i64>() }
    pub fn set_position(self, a1: i64) { self.instance1::<"set_Position", i64, ()>(a1) }
    pub fn read_byte(self) -> i32 { self.virt0::<"ReadByte", i32>() }
    pub fn set_length(self, a1: i64) { self.instance1::<"SetLength", i64, ()>(a1) }
    pub fn write_byte(self, a1: u8) { self.instance1::<"WriteByte", u8, ()>(a1) }
    pub fn new(a1: System::Runtime::InteropServices::SafeBuffer, a2: i64, a3: i64) -> Self { Self::ctor3(a1, a2, a3) }
}
}
pub mod Diagnostics{
pub mod SymbolStore{
pub type ISymbolDocumentWriter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.SymbolStore.ISymbolDocumentWriter">;
use super::super::super::*;
}
pub mod Contracts{
pub type ContractException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractException">;
use super::super::super::*;
impl From<ContractException> for System::Exception {
 fn from(v:ContractException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,ContractException>(v)
}} 
impl ContractException {
    pub fn get_failure(self) -> System::String { self.instance0::<"get_Failure", System::String>() }
    pub fn get_user_message(self) -> System::String { self.instance0::<"get_UserMessage", System::String>() }
    pub fn get_condition(self) -> System::String { self.instance0::<"get_Condition", System::String>() }
}
pub type ContractFailedEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractFailedEventArgs">;
use super::super::super::*;
impl From<ContractFailedEventArgs> for System::EventArgs {
 fn from(v:ContractFailedEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,ContractFailedEventArgs>(v)
}} 
impl ContractFailedEventArgs {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_condition(self) -> System::String { self.instance0::<"get_Condition", System::String>() }
    pub fn get_original_exception(self) -> System::Exception { self.instance0::<"get_OriginalException", System::Exception>() }
    pub fn get_handled(self) -> bool { self.instance0::<"get_Handled", bool>() }
    pub fn set_handled(self) { self.instance0::<"SetHandled", ()>() }
    pub fn get_unwind(self) -> bool { self.instance0::<"get_Unwind", bool>() }
    pub fn set_unwind(self) { self.instance0::<"SetUnwind", ()>() }
}
pub type PureAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.PureAttribute">;
use super::super::super::*;
impl From<PureAttribute> for System::Attribute {
 fn from(v:PureAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,PureAttribute>(v)
}} 
impl PureAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractClassAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractClassAttribute">;
use super::super::super::*;
impl From<ContractClassAttribute> for System::Attribute {
 fn from(v:ContractClassAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractClassAttribute>(v)
}} 
impl ContractClassAttribute {
    pub fn get_type_containing_contracts(self) -> System::Type { self.instance0::<"get_TypeContainingContracts", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type ContractClassForAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractClassForAttribute">;
use super::super::super::*;
impl From<ContractClassForAttribute> for System::Attribute {
 fn from(v:ContractClassForAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractClassForAttribute>(v)
}} 
impl ContractClassForAttribute {
    pub fn get_type_contracts_are_for(self) -> System::Type { self.instance0::<"get_TypeContractsAreFor", System::Type>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type ContractInvariantMethodAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractInvariantMethodAttribute">;
use super::super::super::*;
impl From<ContractInvariantMethodAttribute> for System::Attribute {
 fn from(v:ContractInvariantMethodAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractInvariantMethodAttribute>(v)
}} 
impl ContractInvariantMethodAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractReferenceAssemblyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractReferenceAssemblyAttribute">;
use super::super::super::*;
impl From<ContractReferenceAssemblyAttribute> for System::Attribute {
 fn from(v:ContractReferenceAssemblyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractReferenceAssemblyAttribute>(v)
}} 
impl ContractReferenceAssemblyAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractRuntimeIgnoredAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractRuntimeIgnoredAttribute">;
use super::super::super::*;
impl From<ContractRuntimeIgnoredAttribute> for System::Attribute {
 fn from(v:ContractRuntimeIgnoredAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractRuntimeIgnoredAttribute>(v)
}} 
impl ContractRuntimeIgnoredAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractVerificationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractVerificationAttribute">;
use super::super::super::*;
impl From<ContractVerificationAttribute> for System::Attribute {
 fn from(v:ContractVerificationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractVerificationAttribute>(v)
}} 
impl ContractVerificationAttribute {
    pub fn get_value(self) -> bool { self.instance0::<"get_Value", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ContractPublicPropertyNameAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractPublicPropertyNameAttribute">;
use super::super::super::*;
impl From<ContractPublicPropertyNameAttribute> for System::Attribute {
 fn from(v:ContractPublicPropertyNameAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractPublicPropertyNameAttribute>(v)
}} 
impl ContractPublicPropertyNameAttribute {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ContractArgumentValidatorAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractArgumentValidatorAttribute">;
use super::super::super::*;
impl From<ContractArgumentValidatorAttribute> for System::Attribute {
 fn from(v:ContractArgumentValidatorAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractArgumentValidatorAttribute>(v)
}} 
impl ContractArgumentValidatorAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractAbbreviatorAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractAbbreviatorAttribute">;
use super::super::super::*;
impl From<ContractAbbreviatorAttribute> for System::Attribute {
 fn from(v:ContractAbbreviatorAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractAbbreviatorAttribute>(v)
}} 
impl ContractAbbreviatorAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContractOptionAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.ContractOptionAttribute">;
use super::super::super::*;
impl From<ContractOptionAttribute> for System::Attribute {
 fn from(v:ContractOptionAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContractOptionAttribute>(v)
}} 
impl ContractOptionAttribute {
    pub fn get_category(self) -> System::String { self.instance0::<"get_Category", System::String>() }
    pub fn get_setting(self) -> System::String { self.instance0::<"get_Setting", System::String>() }
    pub fn get_enabled(self) -> bool { self.instance0::<"get_Enabled", bool>() }
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn new(a1: System::String, a2: System::String, a3: bool) -> Self { Self::ctor3(a1, a2, a3) }
}
pub type Contract =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Contracts.Contract">;
use super::super::super::*;
impl From<Contract> for System::Object {
 fn from(v:Contract)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Contract>(v)
}} 
impl Contract {
    pub fn assume(a1: bool) { Self::static1::<"Assume", bool, ()>(a1) }
    pub fn assert(a1: bool) { Self::static1::<"Assert", bool, ()>(a1) }
    pub fn requires(a1: bool) { Self::static1::<"Requires", bool, ()>(a1) }
    pub fn ensures(a1: bool) { Self::static1::<"Ensures", bool, ()>(a1) }
    pub fn invariant(a1: bool) { Self::static1::<"Invariant", bool, ()>(a1) }
    pub fn end_contract_block() { Self::static0::<"EndContractBlock", ()>() }
}
}
pub mod CodeAnalysis{
pub type ConstantExpectedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.ConstantExpectedAttribute">;
use super::super::super::*;
impl From<ConstantExpectedAttribute> for System::Attribute {
 fn from(v:ConstantExpectedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ConstantExpectedAttribute>(v)
}} 
impl ConstantExpectedAttribute {
    pub fn get_min(self) -> System::Object { self.instance0::<"get_Min", System::Object>() }
    pub fn set_min(self, a1: System::Object) { self.instance1::<"set_Min", System::Object, ()>(a1) }
    pub fn get_max(self) -> System::Object { self.instance0::<"get_Max", System::Object>() }
    pub fn set_max(self, a1: System::Object) { self.instance1::<"set_Max", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type DynamicallyAccessedMembersAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.DynamicallyAccessedMembersAttribute">;
use super::super::super::*;
impl From<DynamicallyAccessedMembersAttribute> for System::Attribute {
 fn from(v:DynamicallyAccessedMembersAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DynamicallyAccessedMembersAttribute>(v)
}} 
pub type DynamicDependencyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.DynamicDependencyAttribute">;
use super::super::super::*;
impl From<DynamicDependencyAttribute> for System::Attribute {
 fn from(v:DynamicDependencyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DynamicDependencyAttribute>(v)
}} 
impl DynamicDependencyAttribute {
    pub fn get_member_signature(self) -> System::String { self.instance0::<"get_MemberSignature", System::String>() }
    pub fn get_type(self) -> System::Type { self.instance0::<"get_Type", System::Type>() }
    pub fn get_type_name(self) -> System::String { self.instance0::<"get_TypeName", System::String>() }
    pub fn get_assembly_name(self) -> System::String { self.instance0::<"get_AssemblyName", System::String>() }
    pub fn get_condition(self) -> System::String { self.instance0::<"get_Condition", System::String>() }
    pub fn set_condition(self, a1: System::String) { self.instance1::<"set_Condition", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ExcludeFromCodeCoverageAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.ExcludeFromCodeCoverageAttribute">;
use super::super::super::*;
impl From<ExcludeFromCodeCoverageAttribute> for System::Attribute {
 fn from(v:ExcludeFromCodeCoverageAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ExcludeFromCodeCoverageAttribute>(v)
}} 
impl ExcludeFromCodeCoverageAttribute {
    pub fn get_justification(self) -> System::String { self.instance0::<"get_Justification", System::String>() }
    pub fn set_justification(self, a1: System::String) { self.instance1::<"set_Justification", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ExperimentalAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.ExperimentalAttribute">;
use super::super::super::*;
impl From<ExperimentalAttribute> for System::Attribute {
 fn from(v:ExperimentalAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ExperimentalAttribute>(v)
}} 
impl ExperimentalAttribute {
    pub fn get_diagnostic_id(self) -> System::String { self.instance0::<"get_DiagnosticId", System::String>() }
    pub fn get_url_format(self) -> System::String { self.instance0::<"get_UrlFormat", System::String>() }
    pub fn set_url_format(self, a1: System::String) { self.instance1::<"set_UrlFormat", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type AllowNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.AllowNullAttribute">;
use super::super::super::*;
impl From<AllowNullAttribute> for System::Attribute {
 fn from(v:AllowNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AllowNullAttribute>(v)
}} 
impl AllowNullAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DisallowNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.DisallowNullAttribute">;
use super::super::super::*;
impl From<DisallowNullAttribute> for System::Attribute {
 fn from(v:DisallowNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DisallowNullAttribute>(v)
}} 
impl DisallowNullAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MaybeNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.MaybeNullAttribute">;
use super::super::super::*;
impl From<MaybeNullAttribute> for System::Attribute {
 fn from(v:MaybeNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MaybeNullAttribute>(v)
}} 
impl MaybeNullAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NotNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.NotNullAttribute">;
use super::super::super::*;
impl From<NotNullAttribute> for System::Attribute {
 fn from(v:NotNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NotNullAttribute>(v)
}} 
impl NotNullAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MaybeNullWhenAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.MaybeNullWhenAttribute">;
use super::super::super::*;
impl From<MaybeNullWhenAttribute> for System::Attribute {
 fn from(v:MaybeNullWhenAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MaybeNullWhenAttribute>(v)
}} 
impl MaybeNullWhenAttribute {
    pub fn get_return_value(self) -> bool { self.instance0::<"get_ReturnValue", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type NotNullWhenAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.NotNullWhenAttribute">;
use super::super::super::*;
impl From<NotNullWhenAttribute> for System::Attribute {
 fn from(v:NotNullWhenAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NotNullWhenAttribute>(v)
}} 
impl NotNullWhenAttribute {
    pub fn get_return_value(self) -> bool { self.instance0::<"get_ReturnValue", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type NotNullIfNotNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.NotNullIfNotNullAttribute">;
use super::super::super::*;
impl From<NotNullIfNotNullAttribute> for System::Attribute {
 fn from(v:NotNullIfNotNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NotNullIfNotNullAttribute>(v)
}} 
impl NotNullIfNotNullAttribute {
    pub fn get_parameter_name(self) -> System::String { self.instance0::<"get_ParameterName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type DoesNotReturnAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.DoesNotReturnAttribute">;
use super::super::super::*;
impl From<DoesNotReturnAttribute> for System::Attribute {
 fn from(v:DoesNotReturnAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DoesNotReturnAttribute>(v)
}} 
impl DoesNotReturnAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DoesNotReturnIfAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.DoesNotReturnIfAttribute">;
use super::super::super::*;
impl From<DoesNotReturnIfAttribute> for System::Attribute {
 fn from(v:DoesNotReturnIfAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DoesNotReturnIfAttribute>(v)
}} 
impl DoesNotReturnIfAttribute {
    pub fn get_parameter_value(self) -> bool { self.instance0::<"get_ParameterValue", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type MemberNotNullAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.MemberNotNullAttribute">;
use super::super::super::*;
impl From<MemberNotNullAttribute> for System::Attribute {
 fn from(v:MemberNotNullAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MemberNotNullAttribute>(v)
}} 
impl MemberNotNullAttribute {
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type MemberNotNullWhenAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.MemberNotNullWhenAttribute">;
use super::super::super::*;
impl From<MemberNotNullWhenAttribute> for System::Attribute {
 fn from(v:MemberNotNullWhenAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MemberNotNullWhenAttribute>(v)
}} 
impl MemberNotNullWhenAttribute {
    pub fn get_return_value(self) -> bool { self.instance0::<"get_ReturnValue", bool>() }
    pub fn new(a1: bool, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type UnscopedRefAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.UnscopedRefAttribute">;
use super::super::super::*;
impl From<UnscopedRefAttribute> for System::Attribute {
 fn from(v:UnscopedRefAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnscopedRefAttribute>(v)
}} 
impl UnscopedRefAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type RequiresAssemblyFilesAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.RequiresAssemblyFilesAttribute">;
use super::super::super::*;
impl From<RequiresAssemblyFilesAttribute> for System::Attribute {
 fn from(v:RequiresAssemblyFilesAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiresAssemblyFilesAttribute>(v)
}} 
impl RequiresAssemblyFilesAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type RequiresDynamicCodeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.RequiresDynamicCodeAttribute">;
use super::super::super::*;
impl From<RequiresDynamicCodeAttribute> for System::Attribute {
 fn from(v:RequiresDynamicCodeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiresDynamicCodeAttribute>(v)
}} 
impl RequiresDynamicCodeAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type RequiresUnreferencedCodeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.RequiresUnreferencedCodeAttribute">;
use super::super::super::*;
impl From<RequiresUnreferencedCodeAttribute> for System::Attribute {
 fn from(v:RequiresUnreferencedCodeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,RequiresUnreferencedCodeAttribute>(v)
}} 
impl RequiresUnreferencedCodeAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_url(self) -> System::String { self.instance0::<"get_Url", System::String>() }
    pub fn set_url(self, a1: System::String) { self.instance1::<"set_Url", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SetsRequiredMembersAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.SetsRequiredMembersAttribute">;
use super::super::super::*;
impl From<SetsRequiredMembersAttribute> for System::Attribute {
 fn from(v:SetsRequiredMembersAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SetsRequiredMembersAttribute>(v)
}} 
impl SetsRequiredMembersAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type StringSyntaxAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.StringSyntaxAttribute">;
use super::super::super::*;
impl From<StringSyntaxAttribute> for System::Attribute {
 fn from(v:StringSyntaxAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,StringSyntaxAttribute>(v)
}} 
impl StringSyntaxAttribute {
    pub fn get_syntax(self) -> System::String { self.instance0::<"get_Syntax", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type SuppressMessageAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.SuppressMessageAttribute">;
use super::super::super::*;
impl From<SuppressMessageAttribute> for System::Attribute {
 fn from(v:SuppressMessageAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SuppressMessageAttribute>(v)
}} 
impl SuppressMessageAttribute {
    pub fn get_category(self) -> System::String { self.instance0::<"get_Category", System::String>() }
    pub fn get_check_id(self) -> System::String { self.instance0::<"get_CheckId", System::String>() }
    pub fn get_scope(self) -> System::String { self.instance0::<"get_Scope", System::String>() }
    pub fn set_scope(self, a1: System::String) { self.instance1::<"set_Scope", System::String, ()>(a1) }
    pub fn get_target(self) -> System::String { self.instance0::<"get_Target", System::String>() }
    pub fn set_target(self, a1: System::String) { self.instance1::<"set_Target", System::String, ()>(a1) }
    pub fn get_message_id(self) -> System::String { self.instance0::<"get_MessageId", System::String>() }
    pub fn set_message_id(self, a1: System::String) { self.instance1::<"set_MessageId", System::String, ()>(a1) }
    pub fn get_justification(self) -> System::String { self.instance0::<"get_Justification", System::String>() }
    pub fn set_justification(self, a1: System::String) { self.instance1::<"set_Justification", System::String, ()>(a1) }
    pub fn new(a1: System::String, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
pub type UnconditionalSuppressMessageAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.CodeAnalysis.UnconditionalSuppressMessageAttribute">;
use super::super::super::*;
impl From<UnconditionalSuppressMessageAttribute> for System::Attribute {
 fn from(v:UnconditionalSuppressMessageAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,UnconditionalSuppressMessageAttribute>(v)
}} 
impl UnconditionalSuppressMessageAttribute {
    pub fn get_category(self) -> System::String { self.instance0::<"get_Category", System::String>() }
    pub fn get_check_id(self) -> System::String { self.instance0::<"get_CheckId", System::String>() }
    pub fn get_scope(self) -> System::String { self.instance0::<"get_Scope", System::String>() }
    pub fn set_scope(self, a1: System::String) { self.instance1::<"set_Scope", System::String, ()>(a1) }
    pub fn get_target(self) -> System::String { self.instance0::<"get_Target", System::String>() }
    pub fn set_target(self, a1: System::String) { self.instance1::<"set_Target", System::String, ()>(a1) }
    pub fn get_message_id(self) -> System::String { self.instance0::<"get_MessageId", System::String>() }
    pub fn set_message_id(self, a1: System::String) { self.instance1::<"set_MessageId", System::String, ()>(a1) }
    pub fn get_justification(self) -> System::String { self.instance0::<"get_Justification", System::String>() }
    pub fn set_justification(self, a1: System::String) { self.instance1::<"set_Justification", System::String, ()>(a1) }
    pub fn new(a1: System::String, a2: System::String) -> Self { Self::ctor2(a1, a2) }
}
}
pub mod Tracing{
pub type DiagnosticCounter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.DiagnosticCounter">;
use super::super::super::*;
impl From<DiagnosticCounter> for System::Object {
 fn from(v:DiagnosticCounter)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DiagnosticCounter>(v)
}} 
impl DiagnosticCounter {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn add_metadata(self, a1: System::String, a2: System::String) { self.instance2::<"AddMetadata", System::String, System::String, ()>(a1, a2) }
    pub fn get_display_name(self) -> System::String { self.instance0::<"get_DisplayName", System::String>() }
    pub fn set_display_name(self, a1: System::String) { self.instance1::<"set_DisplayName", System::String, ()>(a1) }
    pub fn get_display_units(self) -> System::String { self.instance0::<"get_DisplayUnits", System::String>() }
    pub fn set_display_units(self, a1: System::String) { self.instance1::<"set_DisplayUnits", System::String, ()>(a1) }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_event_source(self) -> System::Diagnostics::Tracing::EventSource { self.instance0::<"get_EventSource", System::Diagnostics::Tracing::EventSource>() }
}
pub type EventCounter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventCounter">;
use super::super::super::*;
impl From<EventCounter> for System::Diagnostics::Tracing::DiagnosticCounter {
 fn from(v:EventCounter)->System::Diagnostics::Tracing::DiagnosticCounter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Diagnostics::Tracing::DiagnosticCounter,EventCounter>(v)
}} 
impl EventCounter {
    pub fn write_metric(self, a1: f32) { self.instance1::<"WriteMetric", f32, ()>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new(a1: System::String, a2: System::Diagnostics::Tracing::EventSource) -> Self { Self::ctor2(a1, a2) }
}
pub type EventSource =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventSource">;
use super::super::super::*;
impl From<EventSource> for System::Object {
 fn from(v:EventSource)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EventSource>(v)
}} 
impl EventSource {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn is_enabled(self) -> bool { self.instance0::<"IsEnabled", bool>() }
    pub fn generate_manifest(a1: System::Type, a2: System::String) -> System::String { Self::static2::<"GenerateManifest", System::Type, System::String, System::String>(a1, a2) }
    pub fn get_construction_exception(self) -> System::Exception { self.instance0::<"get_ConstructionException", System::Exception>() }
    pub fn get_trait(self, a1: System::String) -> System::String { self.instance1::<"GetTrait", System::String, System::String>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn write(self, a1: System::String) { self.instance1::<"Write", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type EventListener =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventListener">;
use super::super::super::*;
impl From<EventListener> for System::Object {
 fn from(v:EventListener)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EventListener>(v)
}} 
impl EventListener {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn disable_events(self, a1: System::Diagnostics::Tracing::EventSource) { self.instance1::<"DisableEvents", System::Diagnostics::Tracing::EventSource, ()>(a1) }
}
pub type EventCommandEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventCommandEventArgs">;
use super::super::super::*;
impl From<EventCommandEventArgs> for System::EventArgs {
 fn from(v:EventCommandEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,EventCommandEventArgs>(v)
}} 
impl EventCommandEventArgs {
    pub fn enable_event(self, a1: i32) -> bool { self.instance1::<"EnableEvent", i32, bool>(a1) }
    pub fn disable_event(self, a1: i32) -> bool { self.instance1::<"DisableEvent", i32, bool>(a1) }
}
pub type EventSourceCreatedEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventSourceCreatedEventArgs">;
use super::super::super::*;
impl From<EventSourceCreatedEventArgs> for System::EventArgs {
 fn from(v:EventSourceCreatedEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,EventSourceCreatedEventArgs>(v)
}} 
impl EventSourceCreatedEventArgs {
    pub fn get_event_source(self) -> System::Diagnostics::Tracing::EventSource { self.instance0::<"get_EventSource", System::Diagnostics::Tracing::EventSource>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventWrittenEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventWrittenEventArgs">;
use super::super::super::*;
impl From<EventWrittenEventArgs> for System::EventArgs {
 fn from(v:EventWrittenEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,EventWrittenEventArgs>(v)
}} 
impl EventWrittenEventArgs {
    pub fn get_event_name(self) -> System::String { self.instance0::<"get_EventName", System::String>() }
    pub fn get_event_id(self) -> i32 { self.instance0::<"get_EventId", i32>() }
    pub fn get_event_source(self) -> System::Diagnostics::Tracing::EventSource { self.instance0::<"get_EventSource", System::Diagnostics::Tracing::EventSource>() }
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_version(self) -> u8 { self.instance0::<"get_Version", u8>() }
    pub fn get_osthread_id(self) -> i64 { self.instance0::<"get_OSThreadId", i64>() }
}
pub type EventSourceAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventSourceAttribute">;
use super::super::super::*;
impl From<EventSourceAttribute> for System::Attribute {
 fn from(v:EventSourceAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EventSourceAttribute>(v)
}} 
impl EventSourceAttribute {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn get_guid(self) -> System::String { self.instance0::<"get_Guid", System::String>() }
    pub fn set_guid(self, a1: System::String) { self.instance1::<"set_Guid", System::String, ()>(a1) }
    pub fn get_localization_resources(self) -> System::String { self.instance0::<"get_LocalizationResources", System::String>() }
    pub fn set_localization_resources(self, a1: System::String) { self.instance1::<"set_LocalizationResources", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventAttribute">;
use super::super::super::*;
impl From<EventAttribute> for System::Attribute {
 fn from(v:EventAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EventAttribute>(v)
}} 
impl EventAttribute {
    pub fn get_event_id(self) -> i32 { self.instance0::<"get_EventId", i32>() }
    pub fn get_version(self) -> u8 { self.instance0::<"get_Version", u8>() }
    pub fn set_version(self, a1: u8) { self.instance1::<"set_Version", u8, ()>(a1) }
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn set_message(self, a1: System::String) { self.instance1::<"set_Message", System::String, ()>(a1) }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type NonEventAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.NonEventAttribute">;
use super::super::super::*;
impl From<NonEventAttribute> for System::Attribute {
 fn from(v:NonEventAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NonEventAttribute>(v)
}} 
impl NonEventAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventSourceException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventSourceException">;
use super::super::super::*;
impl From<EventSourceException> for System::Exception {
 fn from(v:EventSourceException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,EventSourceException>(v)
}} 
impl EventSourceException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type IncrementingEventCounter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.IncrementingEventCounter">;
use super::super::super::*;
impl From<IncrementingEventCounter> for System::Diagnostics::Tracing::DiagnosticCounter {
 fn from(v:IncrementingEventCounter)->System::Diagnostics::Tracing::DiagnosticCounter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Diagnostics::Tracing::DiagnosticCounter,IncrementingEventCounter>(v)
}} 
impl IncrementingEventCounter {
    pub fn increment(self, a1: f64) { self.instance1::<"Increment", f64, ()>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new(a1: System::String, a2: System::Diagnostics::Tracing::EventSource) -> Self { Self::ctor2(a1, a2) }
}
pub type IncrementingPollingCounter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.IncrementingPollingCounter">;
use super::super::super::*;
impl From<IncrementingPollingCounter> for System::Diagnostics::Tracing::DiagnosticCounter {
 fn from(v:IncrementingPollingCounter)->System::Diagnostics::Tracing::DiagnosticCounter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Diagnostics::Tracing::DiagnosticCounter,IncrementingPollingCounter>(v)
}} 
impl IncrementingPollingCounter {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type PollingCounter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.PollingCounter">;
use super::super::super::*;
impl From<PollingCounter> for System::Diagnostics::Tracing::DiagnosticCounter {
 fn from(v:PollingCounter)->System::Diagnostics::Tracing::DiagnosticCounter{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Diagnostics::Tracing::DiagnosticCounter,PollingCounter>(v)
}} 
impl PollingCounter {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type EventDataAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventDataAttribute">;
use super::super::super::*;
impl From<EventDataAttribute> for System::Attribute {
 fn from(v:EventDataAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EventDataAttribute>(v)
}} 
impl EventDataAttribute {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventFieldAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventFieldAttribute">;
use super::super::super::*;
impl From<EventFieldAttribute> for System::Attribute {
 fn from(v:EventFieldAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EventFieldAttribute>(v)
}} 
impl EventFieldAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventIgnoreAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Tracing.EventIgnoreAttribute">;
use super::super::super::*;
impl From<EventIgnoreAttribute> for System::Attribute {
 fn from(v:EventIgnoreAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,EventIgnoreAttribute>(v)
}} 
impl EventIgnoreAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
}
pub type Debugger =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Debugger">;
use super::super::*;
impl From<Debugger> for System::Object {
 fn from(v:Debugger)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Debugger>(v)
}} 
impl Debugger {
    pub fn r#break() { Self::static0::<"Break", ()>() }
    pub fn launch() -> bool { Self::static0::<"Launch", bool>() }
    pub fn notify_of_cross_thread_dependency() { Self::static0::<"NotifyOfCrossThreadDependency", ()>() }
    pub fn get_is_attached() -> bool { Self::static0::<"get_IsAttached", bool>() }
    pub fn is_logging() -> bool { Self::static0::<"IsLogging", bool>() }
}
pub type StackFrame =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.StackFrame">;
use super::super::*;
impl From<StackFrame> for System::Object {
 fn from(v:StackFrame)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StackFrame>(v)
}} 
impl StackFrame {
    pub fn get_method(self) -> System::Reflection::MethodBase { self.virt0::<"GetMethod", System::Reflection::MethodBase>() }
    pub fn get_native_offset(self) -> i32 { self.virt0::<"GetNativeOffset", i32>() }
    pub fn get_iloffset(self) -> i32 { self.virt0::<"GetILOffset", i32>() }
    pub fn get_file_name(self) -> System::String { self.virt0::<"GetFileName", System::String>() }
    pub fn get_file_line_number(self) -> i32 { self.virt0::<"GetFileLineNumber", i32>() }
    pub fn get_file_column_number(self) -> i32 { self.virt0::<"GetFileColumnNumber", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type StackTrace =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.StackTrace">;
use super::super::*;
impl From<StackTrace> for System::Object {
 fn from(v:StackTrace)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StackTrace>(v)
}} 
impl StackTrace {
    pub fn get_frame_count(self) -> i32 { self.virt0::<"get_FrameCount", i32>() }
    pub fn get_frame(self, a1: i32) -> System::Diagnostics::StackFrame { self.instance1::<"GetFrame", i32, System::Diagnostics::StackFrame>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ConditionalAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.ConditionalAttribute">;
use super::super::*;
impl From<ConditionalAttribute> for System::Attribute {
 fn from(v:ConditionalAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ConditionalAttribute>(v)
}} 
impl ConditionalAttribute {
    pub fn get_condition_string(self) -> System::String { self.instance0::<"get_ConditionString", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type Debug =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Debug">;
use super::super::*;
impl From<Debug> for System::Object {
 fn from(v:Debug)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Debug>(v)
}} 
impl Debug {
    pub fn set_provider(a1: System::Diagnostics::DebugProvider) -> System::Diagnostics::DebugProvider { Self::static1::<"SetProvider", System::Diagnostics::DebugProvider, System::Diagnostics::DebugProvider>(a1) }
    pub fn get_auto_flush() -> bool { Self::static0::<"get_AutoFlush", bool>() }
    pub fn set_auto_flush(a1: bool) { Self::static1::<"set_AutoFlush", bool, ()>(a1) }
    pub fn get_indent_level() -> i32 { Self::static0::<"get_IndentLevel", i32>() }
    pub fn set_indent_level(a1: i32) { Self::static1::<"set_IndentLevel", i32, ()>(a1) }
    pub fn get_indent_size() -> i32 { Self::static0::<"get_IndentSize", i32>() }
    pub fn set_indent_size(a1: i32) { Self::static1::<"set_IndentSize", i32, ()>(a1) }
    pub fn close() { Self::static0::<"Close", ()>() }
    pub fn flush() { Self::static0::<"Flush", ()>() }
    pub fn indent() { Self::static0::<"Indent", ()>() }
    pub fn unindent() { Self::static0::<"Unindent", ()>() }
    pub fn print(a1: System::String) { Self::static1::<"Print", System::String, ()>(a1) }
    pub fn assert(a1: bool) { Self::static1::<"Assert", bool, ()>(a1) }
    pub fn fail(a1: System::String) { Self::static1::<"Fail", System::String, ()>(a1) }
    pub fn write_line(a1: System::String) { Self::static1::<"WriteLine", System::String, ()>(a1) }
    pub fn write(a1: System::String) { Self::static1::<"Write", System::String, ()>(a1) }
    pub fn write_if(a1: bool, a2: System::String) { Self::static2::<"WriteIf", bool, System::String, ()>(a1, a2) }
    pub fn write_line_if(a1: bool, a2: System::Object) { Self::static2::<"WriteLineIf", bool, System::Object, ()>(a1, a2) }
}
pub type DebuggableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggableAttribute">;
use super::super::*;
impl From<DebuggableAttribute> for System::Attribute {
 fn from(v:DebuggableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggableAttribute>(v)
}} 
impl DebuggableAttribute {
    pub fn get_is_jittracking_enabled(self) -> bool { self.instance0::<"get_IsJITTrackingEnabled", bool>() }
    pub fn get_is_jitoptimizer_disabled(self) -> bool { self.instance0::<"get_IsJITOptimizerDisabled", bool>() }
    pub fn new(a1: bool, a2: bool) -> Self { Self::ctor2(a1, a2) }
}
pub type DebuggerBrowsableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerBrowsableAttribute">;
use super::super::*;
impl From<DebuggerBrowsableAttribute> for System::Attribute {
 fn from(v:DebuggerBrowsableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerBrowsableAttribute>(v)
}} 
pub type DebuggerDisplayAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerDisplayAttribute">;
use super::super::*;
impl From<DebuggerDisplayAttribute> for System::Attribute {
 fn from(v:DebuggerDisplayAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerDisplayAttribute>(v)
}} 
impl DebuggerDisplayAttribute {
    pub fn get_value(self) -> System::String { self.instance0::<"get_Value", System::String>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn set_name(self, a1: System::String) { self.instance1::<"set_Name", System::String, ()>(a1) }
    pub fn get_type(self) -> System::String { self.instance0::<"get_Type", System::String>() }
    pub fn set_type(self, a1: System::String) { self.instance1::<"set_Type", System::String, ()>(a1) }
    pub fn get_target(self) -> System::Type { self.instance0::<"get_Target", System::Type>() }
    pub fn set_target(self, a1: System::Type) { self.instance1::<"set_Target", System::Type, ()>(a1) }
    pub fn get_target_type_name(self) -> System::String { self.instance0::<"get_TargetTypeName", System::String>() }
    pub fn set_target_type_name(self, a1: System::String) { self.instance1::<"set_TargetTypeName", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type DebuggerHiddenAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerHiddenAttribute">;
use super::super::*;
impl From<DebuggerHiddenAttribute> for System::Attribute {
 fn from(v:DebuggerHiddenAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerHiddenAttribute>(v)
}} 
impl DebuggerHiddenAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DebuggerNonUserCodeAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerNonUserCodeAttribute">;
use super::super::*;
impl From<DebuggerNonUserCodeAttribute> for System::Attribute {
 fn from(v:DebuggerNonUserCodeAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerNonUserCodeAttribute>(v)
}} 
impl DebuggerNonUserCodeAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DebuggerStepperBoundaryAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerStepperBoundaryAttribute">;
use super::super::*;
impl From<DebuggerStepperBoundaryAttribute> for System::Attribute {
 fn from(v:DebuggerStepperBoundaryAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerStepperBoundaryAttribute>(v)
}} 
impl DebuggerStepperBoundaryAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DebuggerStepThroughAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerStepThroughAttribute">;
use super::super::*;
impl From<DebuggerStepThroughAttribute> for System::Attribute {
 fn from(v:DebuggerStepThroughAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerStepThroughAttribute>(v)
}} 
impl DebuggerStepThroughAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DebuggerTypeProxyAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerTypeProxyAttribute">;
use super::super::*;
impl From<DebuggerTypeProxyAttribute> for System::Attribute {
 fn from(v:DebuggerTypeProxyAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerTypeProxyAttribute>(v)
}} 
impl DebuggerTypeProxyAttribute {
    pub fn get_proxy_type_name(self) -> System::String { self.instance0::<"get_ProxyTypeName", System::String>() }
    pub fn get_target(self) -> System::Type { self.instance0::<"get_Target", System::Type>() }
    pub fn set_target(self, a1: System::Type) { self.instance1::<"set_Target", System::Type, ()>(a1) }
    pub fn get_target_type_name(self) -> System::String { self.instance0::<"get_TargetTypeName", System::String>() }
    pub fn set_target_type_name(self, a1: System::String) { self.instance1::<"set_TargetTypeName", System::String, ()>(a1) }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
pub type DebuggerVisualizerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebuggerVisualizerAttribute">;
use super::super::*;
impl From<DebuggerVisualizerAttribute> for System::Attribute {
 fn from(v:DebuggerVisualizerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,DebuggerVisualizerAttribute>(v)
}} 
impl DebuggerVisualizerAttribute {
    pub fn get_visualizer_object_source_type_name(self) -> System::String { self.instance0::<"get_VisualizerObjectSourceTypeName", System::String>() }
    pub fn get_visualizer_type_name(self) -> System::String { self.instance0::<"get_VisualizerTypeName", System::String>() }
    pub fn get_description(self) -> System::String { self.instance0::<"get_Description", System::String>() }
    pub fn set_description(self, a1: System::String) { self.instance1::<"set_Description", System::String, ()>(a1) }
    pub fn get_target(self) -> System::Type { self.instance0::<"get_Target", System::Type>() }
    pub fn set_target(self, a1: System::Type) { self.instance1::<"set_Target", System::Type, ()>(a1) }
    pub fn get_target_type_name(self) -> System::String { self.instance0::<"get_TargetTypeName", System::String>() }
    pub fn set_target_type_name(self, a1: System::String) { self.instance1::<"set_TargetTypeName", System::String, ()>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type DebugProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.DebugProvider">;
use super::super::*;
impl From<DebugProvider> for System::Object {
 fn from(v:DebugProvider)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DebugProvider>(v)
}} 
impl DebugProvider {
    pub fn fail(self, a1: System::String, a2: System::String) { self.instance2::<"Fail", System::String, System::String, ()>(a1, a2) }
    pub fn write(self, a1: System::String) { self.instance1::<"Write", System::String, ()>(a1) }
    pub fn write_line(self, a1: System::String) { self.instance1::<"WriteLine", System::String, ()>(a1) }
    pub fn on_indent_level_changed(self, a1: i32) { self.instance1::<"OnIndentLevelChanged", i32, ()>(a1) }
    pub fn on_indent_size_changed(self, a1: i32) { self.instance1::<"OnIndentSizeChanged", i32, ()>(a1) }
    pub fn write_core(a1: System::String) { Self::static1::<"WriteCore", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type StackFrameExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.StackFrameExtensions">;
use super::super::*;
impl From<StackFrameExtensions> for System::Object {
 fn from(v:StackFrameExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StackFrameExtensions>(v)
}} 
impl StackFrameExtensions {
    pub fn has_native_image(a1: System::Diagnostics::StackFrame) -> bool { Self::static1::<"HasNativeImage", System::Diagnostics::StackFrame, bool>(a1) }
    pub fn has_method(a1: System::Diagnostics::StackFrame) -> bool { Self::static1::<"HasMethod", System::Diagnostics::StackFrame, bool>(a1) }
    pub fn has_iloffset(a1: System::Diagnostics::StackFrame) -> bool { Self::static1::<"HasILOffset", System::Diagnostics::StackFrame, bool>(a1) }
    pub fn has_source(a1: System::Diagnostics::StackFrame) -> bool { Self::static1::<"HasSource", System::Diagnostics::StackFrame, bool>(a1) }
    pub fn get_native_ip(a1: System::Diagnostics::StackFrame) -> isize { Self::static1::<"GetNativeIP", System::Diagnostics::StackFrame, isize>(a1) }
    pub fn get_native_image_base(a1: System::Diagnostics::StackFrame) -> isize { Self::static1::<"GetNativeImageBase", System::Diagnostics::StackFrame, isize>(a1) }
}
pub type StackTraceHiddenAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.StackTraceHiddenAttribute">;
use super::super::*;
impl From<StackTraceHiddenAttribute> for System::Attribute {
 fn from(v:StackTraceHiddenAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,StackTraceHiddenAttribute>(v)
}} 
impl StackTraceHiddenAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Stopwatch =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.Stopwatch">;
use super::super::*;
impl From<Stopwatch> for System::Object {
 fn from(v:Stopwatch)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Stopwatch>(v)
}} 
impl Stopwatch {
    pub fn start(self) { self.instance0::<"Start", ()>() }
    pub fn start_new() -> System::Diagnostics::Stopwatch { Self::static0::<"StartNew", System::Diagnostics::Stopwatch>() }
    pub fn stop(self) { self.instance0::<"Stop", ()>() }
    pub fn reset(self) { self.instance0::<"Reset", ()>() }
    pub fn restart(self) { self.instance0::<"Restart", ()>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_is_running(self) -> bool { self.instance0::<"get_IsRunning", bool>() }
    pub fn get_elapsed_milliseconds(self) -> i64 { self.instance0::<"get_ElapsedMilliseconds", i64>() }
    pub fn get_elapsed_ticks(self) -> i64 { self.instance0::<"get_ElapsedTicks", i64>() }
    pub fn get_timestamp() -> i64 { Self::static0::<"GetTimestamp", i64>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnreachableException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Diagnostics.UnreachableException">;
use super::super::*;
impl From<UnreachableException> for System::Exception {
 fn from(v:UnreachableException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,UnreachableException>(v)
}} 
impl UnreachableException {
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod Collections{
pub mod Generic{
pub type ByteEqualityComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.ByteEqualityComparer">;
use super::super::super::*;
impl ByteEqualityComparer {
    pub fn equals(self, a1: u8, a2: u8) -> bool { self.instance2::<"Equals", u8, u8, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: u8) -> i32 { self.instance1::<"GetHashCode", u8, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type CollectionExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.CollectionExtensions">;
use super::super::super::*;
impl From<CollectionExtensions> for System::Object {
 fn from(v:CollectionExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CollectionExtensions>(v)
}} 
pub type KeyNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.KeyNotFoundException">;
use super::super::super::*;
impl From<KeyNotFoundException> for System::SystemException {
 fn from(v:KeyNotFoundException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,KeyNotFoundException>(v)
}} 
impl KeyNotFoundException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type KeyValuePair =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.KeyValuePair">;
use super::super::super::*;
impl From<KeyValuePair> for System::Object {
 fn from(v:KeyValuePair)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,KeyValuePair>(v)
}} 
pub type ReferenceEqualityComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.ReferenceEqualityComparer">;
use super::super::super::*;
impl From<ReferenceEqualityComparer> for System::Object {
 fn from(v:ReferenceEqualityComparer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ReferenceEqualityComparer>(v)
}} 
impl ReferenceEqualityComparer {
    pub fn get_instance() -> System::Collections::Generic::ReferenceEqualityComparer { Self::static0::<"get_Instance", System::Collections::Generic::ReferenceEqualityComparer>() }
    pub fn equals(self, a1: System::Object, a2: System::Object) -> bool { self.instance2::<"Equals", System::Object, System::Object, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: System::Object) -> i32 { self.instance1::<"GetHashCode", System::Object, i32>(a1) }
}
pub type NonRandomizedStringEqualityComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Generic.NonRandomizedStringEqualityComparer">;
use super::super::super::*;
impl From<NonRandomizedStringEqualityComparer> for System::Object {
 fn from(v:NonRandomizedStringEqualityComparer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NonRandomizedStringEqualityComparer>(v)
}} 
impl NonRandomizedStringEqualityComparer {
    pub fn equals(self, a1: System::String, a2: System::String) -> bool { self.instance2::<"Equals", System::String, System::String, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: System::String) -> i32 { self.instance1::<"GetHashCode", System::String, i32>(a1) }
}
}
pub mod Concurrent{
pub type Partitioner =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Concurrent","System.Collections.Concurrent.Partitioner">;
use super::super::super::*;
impl From<Partitioner> for System::Object {
 fn from(v:Partitioner)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Partitioner>(v)
}} 
}
pub mod Specialized{
pub type CollectionsUtil =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.Specialized.CollectionsUtil">;
use super::super::super::*;
impl From<CollectionsUtil> for System::Object {
 fn from(v:CollectionsUtil)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CollectionsUtil>(v)
}} 
impl CollectionsUtil {
    pub fn create_case_insensitive_hashtable() -> System::Collections::Hashtable { Self::static0::<"CreateCaseInsensitiveHashtable", System::Collections::Hashtable>() }
    pub fn create_case_insensitive_sorted_list() -> System::Collections::SortedList { Self::static0::<"CreateCaseInsensitiveSortedList", System::Collections::SortedList>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type HybridDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.HybridDictionary">;
use super::super::super::*;
impl From<HybridDictionary> for System::Object {
 fn from(v:HybridDictionary)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,HybridDictionary>(v)
}} 
impl HybridDictionary {
    pub fn get_item(self, a1: System::Object) -> System::Object { self.instance1::<"get_Item", System::Object, System::Object>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Object) { self.instance2::<"set_Item", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type IOrderedDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.IOrderedDictionary">;
use super::super::super::*;
impl IOrderedDictionary {
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
}
pub type ListDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.ListDictionary">;
use super::super::super::*;
impl From<ListDictionary> for System::Object {
 fn from(v:ListDictionary)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ListDictionary>(v)
}} 
impl ListDictionary {
    pub fn get_item(self, a1: System::Object) -> System::Object { self.instance1::<"get_Item", System::Object, System::Object>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Object) { self.instance2::<"set_Item", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type NameObjectCollectionBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.NameObjectCollectionBase">;
use super::super::super::*;
impl From<NameObjectCollectionBase> for System::Object {
 fn from(v:NameObjectCollectionBase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,NameObjectCollectionBase>(v)
}} 
impl NameObjectCollectionBase {
    pub fn on_deserialization(self, a1: System::Object) { self.instance1::<"OnDeserialization", System::Object, ()>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
}
pub type NameValueCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.NameValueCollection">;
use super::super::super::*;
impl From<NameValueCollection> for System::Collections::Specialized::NameObjectCollectionBase {
 fn from(v:NameValueCollection)->System::Collections::Specialized::NameObjectCollectionBase{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Collections::Specialized::NameObjectCollectionBase,NameValueCollection>(v)
}} 
impl NameValueCollection {
    pub fn add(self, a1: System::Collections::Specialized::NameValueCollection) { self.instance1::<"Add", System::Collections::Specialized::NameValueCollection, ()>(a1) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn has_keys(self) -> bool { self.instance0::<"HasKeys", bool>() }
    pub fn get(self, a1: System::String) -> System::String { self.instance1::<"Get", System::String, System::String>(a1) }
    pub fn set(self, a1: System::String, a2: System::String) { self.instance2::<"Set", System::String, System::String, ()>(a1, a2) }
    pub fn remove(self, a1: System::String) { self.instance1::<"Remove", System::String, ()>(a1) }
    pub fn get_item(self, a1: System::String) -> System::String { self.instance1::<"get_Item", System::String, System::String>(a1) }
    pub fn set_item(self, a1: System::String, a2: System::String) { self.instance2::<"set_Item", System::String, System::String, ()>(a1, a2) }
    pub fn get_key(self, a1: i32) -> System::String { self.instance1::<"GetKey", i32, System::String>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type OrderedDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.OrderedDictionary">;
use super::super::super::*;
impl From<OrderedDictionary> for System::Object {
 fn from(v:OrderedDictionary)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,OrderedDictionary>(v)
}} 
impl OrderedDictionary {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_item(self, a1: i32) -> System::Object { self.instance1::<"get_Item", i32, System::Object>(a1) }
    pub fn set_item(self, a1: i32, a2: System::Object) { self.instance2::<"set_Item", i32, System::Object, ()>(a1, a2) }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn as_read_only(self) -> System::Collections::Specialized::OrderedDictionary { self.instance0::<"AsReadOnly", System::Collections::Specialized::OrderedDictionary>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type StringCollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.StringCollection">;
use super::super::super::*;
impl From<StringCollection> for System::Object {
 fn from(v:StringCollection)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringCollection>(v)
}} 
impl StringCollection {
    pub fn get_item(self, a1: i32) -> System::String { self.instance1::<"get_Item", i32, System::String>(a1) }
    pub fn set_item(self, a1: i32, a2: System::String) { self.instance2::<"set_Item", i32, System::String, ()>(a1, a2) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn add(self, a1: System::String) -> i32 { self.instance1::<"Add", System::String, i32>(a1) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn contains(self, a1: System::String) -> bool { self.instance1::<"Contains", System::String, bool>(a1) }
    pub fn get_enumerator(self) -> System::Collections::Specialized::StringEnumerator { self.instance0::<"GetEnumerator", System::Collections::Specialized::StringEnumerator>() }
    pub fn index_of(self, a1: System::String) -> i32 { self.instance1::<"IndexOf", System::String, i32>(a1) }
    pub fn insert(self, a1: i32, a2: System::String) { self.instance2::<"Insert", i32, System::String, ()>(a1, a2) }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn remove(self, a1: System::String) { self.instance1::<"Remove", System::String, ()>(a1) }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type StringEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.StringEnumerator">;
use super::super::super::*;
impl From<StringEnumerator> for System::Object {
 fn from(v:StringEnumerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringEnumerator>(v)
}} 
impl StringEnumerator {
    pub fn get_current(self) -> System::String { self.instance0::<"get_Current", System::String>() }
    pub fn move_next(self) -> bool { self.instance0::<"MoveNext", bool>() }
    pub fn reset(self) { self.instance0::<"Reset", ()>() }
}
pub type StringDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.Specialized","System.Collections.Specialized.StringDictionary">;
use super::super::super::*;
impl From<StringDictionary> for System::Object {
 fn from(v:StringDictionary)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringDictionary>(v)
}} 
impl StringDictionary {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_item(self, a1: System::String) -> System::String { self.instance1::<"get_Item", System::String, System::String>(a1) }
    pub fn set_item(self, a1: System::String, a2: System::String) { self.instance2::<"set_Item", System::String, System::String, ()>(a1, a2) }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn add(self, a1: System::String, a2: System::String) { self.instance2::<"Add", System::String, System::String, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn contains_key(self, a1: System::String) -> bool { self.instance1::<"ContainsKey", System::String, bool>(a1) }
    pub fn contains_value(self, a1: System::String) -> bool { self.instance1::<"ContainsValue", System::String, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn remove(self, a1: System::String) { self.instance1::<"Remove", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type INotifyCollectionChanged =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Collections.Specialized.INotifyCollectionChanged">;
use super::super::super::*;
pub type NotifyCollectionChangedEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Collections.Specialized.NotifyCollectionChangedEventArgs">;
use super::super::super::*;
impl From<NotifyCollectionChangedEventArgs> for System::EventArgs {
 fn from(v:NotifyCollectionChangedEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,NotifyCollectionChangedEventArgs>(v)
}} 
impl NotifyCollectionChangedEventArgs {
    pub fn get_new_items(self) -> System::Collections::IList { self.instance0::<"get_NewItems", System::Collections::IList>() }
    pub fn get_old_items(self) -> System::Collections::IList { self.instance0::<"get_OldItems", System::Collections::IList>() }
    pub fn get_new_starting_index(self) -> i32 { self.instance0::<"get_NewStartingIndex", i32>() }
    pub fn get_old_starting_index(self) -> i32 { self.instance0::<"get_OldStartingIndex", i32>() }
}
pub type NotifyCollectionChangedEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Collections.Specialized.NotifyCollectionChangedEventHandler">;
use super::super::super::*;
impl From<NotifyCollectionChangedEventHandler> for System::MulticastDelegate {
 fn from(v:NotifyCollectionChangedEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,NotifyCollectionChangedEventHandler>(v)
}} 
impl NotifyCollectionChangedEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::Collections::Specialized::NotifyCollectionChangedEventArgs) { self.instance2::<"Invoke", System::Object, System::Collections::Specialized::NotifyCollectionChangedEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
}
pub type ArrayList =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.ArrayList">;
use super::super::*;
impl From<ArrayList> for System::Object {
 fn from(v:ArrayList)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ArrayList>(v)
}} 
impl ArrayList {
    pub fn get_capacity(self) -> i32 { self.virt0::<"get_Capacity", i32>() }
    pub fn set_capacity(self, a1: i32) { self.instance1::<"set_Capacity", i32, ()>(a1) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_item(self, a1: i32) -> System::Object { self.instance1::<"get_Item", i32, System::Object>(a1) }
    pub fn set_item(self, a1: i32, a2: System::Object) { self.instance2::<"set_Item", i32, System::Object, ()>(a1, a2) }
    pub fn adapter(a1: System::Collections::IList) -> System::Collections::ArrayList { Self::static1::<"Adapter", System::Collections::IList, System::Collections::ArrayList>(a1) }
    pub fn add(self, a1: System::Object) -> i32 { self.instance1::<"Add", System::Object, i32>(a1) }
    pub fn add_range(self, a1: System::Collections::ICollection) { self.instance1::<"AddRange", System::Collections::ICollection, ()>(a1) }
    pub fn binary_search(self, a1: System::Object) -> i32 { self.instance1::<"BinarySearch", System::Object, i32>(a1) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array) { self.instance1::<"CopyTo", System::Array, ()>(a1) }
    pub fn fixed_size(a1: System::Collections::IList) -> System::Collections::IList { Self::static1::<"FixedSize", System::Collections::IList, System::Collections::IList>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn index_of(self, a1: System::Object) -> i32 { self.instance1::<"IndexOf", System::Object, i32>(a1) }
    pub fn insert(self, a1: i32, a2: System::Object) { self.instance2::<"Insert", i32, System::Object, ()>(a1, a2) }
    pub fn insert_range(self, a1: i32, a2: System::Collections::ICollection) { self.instance2::<"InsertRange", i32, System::Collections::ICollection, ()>(a1, a2) }
    pub fn last_index_of(self, a1: System::Object) -> i32 { self.instance1::<"LastIndexOf", System::Object, i32>(a1) }
    pub fn read_only(a1: System::Collections::IList) -> System::Collections::IList { Self::static1::<"ReadOnly", System::Collections::IList, System::Collections::IList>(a1) }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn remove_range(self, a1: i32, a2: i32) { self.instance2::<"RemoveRange", i32, i32, ()>(a1, a2) }
    pub fn repeat(a1: System::Object, a2: i32) -> System::Collections::ArrayList { Self::static2::<"Repeat", System::Object, i32, System::Collections::ArrayList>(a1, a2) }
    pub fn reverse(self) { self.virt0::<"Reverse", ()>() }
    pub fn set_range(self, a1: i32, a2: System::Collections::ICollection) { self.instance2::<"SetRange", i32, System::Collections::ICollection, ()>(a1, a2) }
    pub fn get_range(self, a1: i32, a2: i32) -> System::Collections::ArrayList { self.instance2::<"GetRange", i32, i32, System::Collections::ArrayList>(a1, a2) }
    pub fn sort(self) { self.virt0::<"Sort", ()>() }
    pub fn synchronized(a1: System::Collections::IList) -> System::Collections::IList { Self::static1::<"Synchronized", System::Collections::IList, System::Collections::IList>(a1) }
    pub fn to_array(self, a1: System::Type) -> System::Array { self.instance1::<"ToArray", System::Type, System::Array>(a1) }
    pub fn trim_to_size(self) { self.virt0::<"TrimToSize", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Comparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Comparer">;
use super::super::*;
impl From<Comparer> for System::Object {
 fn from(v:Comparer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Comparer>(v)
}} 
impl Comparer {
    pub fn compare(self, a1: System::Object, a2: System::Object) -> i32 { self.instance2::<"Compare", System::Object, System::Object, i32>(a1, a2) }
    pub fn new(a1: System::Globalization::CultureInfo) -> Self { Self::ctor1(a1) }
}
pub type Hashtable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.Hashtable">;
use super::super::*;
impl From<Hashtable> for System::Object {
 fn from(v:Hashtable)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Hashtable>(v)
}} 
impl Hashtable {
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn contains_key(self, a1: System::Object) -> bool { self.instance1::<"ContainsKey", System::Object, bool>(a1) }
    pub fn contains_value(self, a1: System::Object) -> bool { self.instance1::<"ContainsValue", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_item(self, a1: System::Object) -> System::Object { self.instance1::<"get_Item", System::Object, System::Object>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Object) { self.instance2::<"set_Item", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn synchronized(a1: System::Collections::Hashtable) -> System::Collections::Hashtable { Self::static1::<"Synchronized", System::Collections::Hashtable, System::Collections::Hashtable>(a1) }
    pub fn on_deserialization(self, a1: System::Object) { self.instance1::<"OnDeserialization", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ICollection =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.ICollection">;
use super::super::*;
impl ICollection {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
}
pub type IComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IComparer">;
use super::super::*;
pub type IDictionary =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IDictionary">;
use super::super::*;
impl IDictionary {
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
}
pub type IDictionaryEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IDictionaryEnumerator">;
use super::super::*;
impl IDictionaryEnumerator {
    pub fn get_key(self) -> System::Object { self.virt0::<"get_Key", System::Object>() }
    pub fn get_value(self) -> System::Object { self.virt0::<"get_Value", System::Object>() }
}
pub type IEnumerable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IEnumerable">;
use super::super::*;
impl IEnumerable {
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
}
pub type IEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IEnumerator">;
use super::super::*;
impl IEnumerator {
    pub fn move_next(self) -> bool { self.virt0::<"MoveNext", bool>() }
    pub fn get_current(self) -> System::Object { self.virt0::<"get_Current", System::Object>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type IEqualityComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IEqualityComparer">;
use super::super::*;
pub type IHashCodeProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IHashCodeProvider">;
use super::super::*;
pub type IList =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IList">;
use super::super::*;
impl IList {
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
}
pub type IStructuralComparable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IStructuralComparable">;
use super::super::*;
pub type IStructuralEquatable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.IStructuralEquatable">;
use super::super::*;
pub type ListDictionaryInternal =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Collections.ListDictionaryInternal">;
use super::super::*;
impl From<ListDictionaryInternal> for System::Object {
 fn from(v:ListDictionaryInternal)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ListDictionaryInternal>(v)
}} 
impl ListDictionaryInternal {
    pub fn get_item(self, a1: System::Object) -> System::Object { self.instance1::<"get_Item", System::Object, System::Object>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Object) { self.instance2::<"set_Item", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type BitArray =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections","System.Collections.BitArray">;
use super::super::*;
impl From<BitArray> for System::Object {
 fn from(v:BitArray)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BitArray>(v)
}} 
impl BitArray {
    pub fn get_item(self, a1: i32) -> bool { self.instance1::<"get_Item", i32, bool>(a1) }
    pub fn set_item(self, a1: i32, a2: bool) { self.instance2::<"set_Item", i32, bool, ()>(a1, a2) }
    pub fn get(self, a1: i32) -> bool { self.instance1::<"Get", i32, bool>(a1) }
    pub fn set(self, a1: i32, a2: bool) { self.instance2::<"Set", i32, bool, ()>(a1, a2) }
    pub fn set_all(self, a1: bool) { self.instance1::<"SetAll", bool, ()>(a1) }
    pub fn and(self, a1: System::Collections::BitArray) -> System::Collections::BitArray { self.instance1::<"And", System::Collections::BitArray, System::Collections::BitArray>(a1) }
    pub fn or(self, a1: System::Collections::BitArray) -> System::Collections::BitArray { self.instance1::<"Or", System::Collections::BitArray, System::Collections::BitArray>(a1) }
    pub fn xor(self, a1: System::Collections::BitArray) -> System::Collections::BitArray { self.instance1::<"Xor", System::Collections::BitArray, System::Collections::BitArray>(a1) }
    pub fn not(self) -> System::Collections::BitArray { self.instance0::<"Not", System::Collections::BitArray>() }
    pub fn right_shift(self, a1: i32) -> System::Collections::BitArray { self.instance1::<"RightShift", i32, System::Collections::BitArray>(a1) }
    pub fn left_shift(self, a1: i32) -> System::Collections::BitArray { self.instance1::<"LeftShift", i32, System::Collections::BitArray>(a1) }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn set_length(self, a1: i32) { self.instance1::<"set_Length", i32, ()>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn has_all_set(self) -> bool { self.instance0::<"HasAllSet", bool>() }
    pub fn has_any_set(self) -> bool { self.instance0::<"HasAnySet", bool>() }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_is_read_only(self) -> bool { self.instance0::<"get_IsReadOnly", bool>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn new(a1: i32) -> Self { Self::ctor1(a1) }
}
pub type StructuralComparisons =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections","System.Collections.StructuralComparisons">;
use super::super::*;
impl From<StructuralComparisons> for System::Object {
 fn from(v:StructuralComparisons)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StructuralComparisons>(v)
}} 
impl StructuralComparisons {
    pub fn get_structural_comparer() -> System::Collections::IComparer { Self::static0::<"get_StructuralComparer", System::Collections::IComparer>() }
    pub fn get_structural_equality_comparer() -> System::Collections::IEqualityComparer { Self::static0::<"get_StructuralEqualityComparer", System::Collections::IEqualityComparer>() }
}
pub type CaseInsensitiveComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.CaseInsensitiveComparer">;
use super::super::*;
impl From<CaseInsensitiveComparer> for System::Object {
 fn from(v:CaseInsensitiveComparer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CaseInsensitiveComparer>(v)
}} 
impl CaseInsensitiveComparer {
    pub fn get_default() -> System::Collections::CaseInsensitiveComparer { Self::static0::<"get_Default", System::Collections::CaseInsensitiveComparer>() }
    pub fn get_default_invariant() -> System::Collections::CaseInsensitiveComparer { Self::static0::<"get_DefaultInvariant", System::Collections::CaseInsensitiveComparer>() }
    pub fn compare(self, a1: System::Object, a2: System::Object) -> i32 { self.instance2::<"Compare", System::Object, System::Object, i32>(a1, a2) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type CaseInsensitiveHashCodeProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.CaseInsensitiveHashCodeProvider">;
use super::super::*;
impl From<CaseInsensitiveHashCodeProvider> for System::Object {
 fn from(v:CaseInsensitiveHashCodeProvider)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CaseInsensitiveHashCodeProvider>(v)
}} 
impl CaseInsensitiveHashCodeProvider {
    pub fn get_default() -> System::Collections::CaseInsensitiveHashCodeProvider { Self::static0::<"get_Default", System::Collections::CaseInsensitiveHashCodeProvider>() }
    pub fn get_default_invariant() -> System::Collections::CaseInsensitiveHashCodeProvider { Self::static0::<"get_DefaultInvariant", System::Collections::CaseInsensitiveHashCodeProvider>() }
    pub fn get_hash_code(self, a1: System::Object) -> i32 { self.instance1::<"GetHashCode", System::Object, i32>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type CollectionBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.CollectionBase">;
use super::super::*;
impl From<CollectionBase> for System::Object {
 fn from(v:CollectionBase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CollectionBase>(v)
}} 
impl CollectionBase {
    pub fn get_capacity(self) -> i32 { self.instance0::<"get_Capacity", i32>() }
    pub fn set_capacity(self, a1: i32) { self.instance1::<"set_Capacity", i32, ()>(a1) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
}
pub type DictionaryBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.DictionaryBase">;
use super::super::*;
impl From<DictionaryBase> for System::Object {
 fn from(v:DictionaryBase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DictionaryBase>(v)
}} 
impl DictionaryBase {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
}
pub type Queue =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.Queue">;
use super::super::*;
impl From<Queue> for System::Object {
 fn from(v:Queue)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Queue>(v)
}} 
impl Queue {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn enqueue(self, a1: System::Object) { self.instance1::<"Enqueue", System::Object, ()>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn dequeue(self) -> System::Object { self.virt0::<"Dequeue", System::Object>() }
    pub fn peek(self) -> System::Object { self.virt0::<"Peek", System::Object>() }
    pub fn synchronized(a1: System::Collections::Queue) -> System::Collections::Queue { Self::static1::<"Synchronized", System::Collections::Queue, System::Collections::Queue>(a1) }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn trim_to_size(self) { self.virt0::<"TrimToSize", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ReadOnlyCollectionBase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.ReadOnlyCollectionBase">;
use super::super::*;
impl From<ReadOnlyCollectionBase> for System::Object {
 fn from(v:ReadOnlyCollectionBase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ReadOnlyCollectionBase>(v)
}} 
impl ReadOnlyCollectionBase {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
}
pub type SortedList =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.SortedList">;
use super::super::*;
impl From<SortedList> for System::Object {
 fn from(v:SortedList)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SortedList>(v)
}} 
impl SortedList {
    pub fn add(self, a1: System::Object, a2: System::Object) { self.instance2::<"Add", System::Object, System::Object, ()>(a1, a2) }
    pub fn get_capacity(self) -> i32 { self.virt0::<"get_Capacity", i32>() }
    pub fn set_capacity(self, a1: i32) { self.instance1::<"set_Capacity", i32, ()>(a1) }
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_keys(self) -> System::Collections::ICollection { self.virt0::<"get_Keys", System::Collections::ICollection>() }
    pub fn get_values(self) -> System::Collections::ICollection { self.virt0::<"get_Values", System::Collections::ICollection>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn contains_key(self, a1: System::Object) -> bool { self.instance1::<"ContainsKey", System::Object, bool>(a1) }
    pub fn contains_value(self, a1: System::Object) -> bool { self.instance1::<"ContainsValue", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_by_index(self, a1: i32) -> System::Object { self.instance1::<"GetByIndex", i32, System::Object>(a1) }
    pub fn get_enumerator(self) -> System::Collections::IDictionaryEnumerator { self.virt0::<"GetEnumerator", System::Collections::IDictionaryEnumerator>() }
    pub fn get_key(self, a1: i32) -> System::Object { self.instance1::<"GetKey", i32, System::Object>(a1) }
    pub fn get_key_list(self) -> System::Collections::IList { self.virt0::<"GetKeyList", System::Collections::IList>() }
    pub fn get_value_list(self) -> System::Collections::IList { self.virt0::<"GetValueList", System::Collections::IList>() }
    pub fn get_item(self, a1: System::Object) -> System::Object { self.instance1::<"get_Item", System::Object, System::Object>(a1) }
    pub fn set_item(self, a1: System::Object, a2: System::Object) { self.instance2::<"set_Item", System::Object, System::Object, ()>(a1, a2) }
    pub fn index_of_key(self, a1: System::Object) -> i32 { self.instance1::<"IndexOfKey", System::Object, i32>(a1) }
    pub fn index_of_value(self, a1: System::Object) -> i32 { self.instance1::<"IndexOfValue", System::Object, i32>(a1) }
    pub fn remove_at(self, a1: i32) { self.instance1::<"RemoveAt", i32, ()>(a1) }
    pub fn remove(self, a1: System::Object) { self.instance1::<"Remove", System::Object, ()>(a1) }
    pub fn set_by_index(self, a1: i32, a2: System::Object) { self.instance2::<"SetByIndex", i32, System::Object, ()>(a1, a2) }
    pub fn synchronized(a1: System::Collections::SortedList) -> System::Collections::SortedList { Self::static1::<"Synchronized", System::Collections::SortedList, System::Collections::SortedList>(a1) }
    pub fn trim_to_size(self) { self.virt0::<"TrimToSize", ()>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Stack =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Collections.NonGeneric","System.Collections.Stack">;
use super::super::*;
impl From<Stack> for System::Object {
 fn from(v:Stack)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Stack>(v)
}} 
impl Stack {
    pub fn get_count(self) -> i32 { self.virt0::<"get_Count", i32>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn clear(self) { self.virt0::<"Clear", ()>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn contains(self, a1: System::Object) -> bool { self.instance1::<"Contains", System::Object, bool>(a1) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
    pub fn peek(self) -> System::Object { self.virt0::<"Peek", System::Object>() }
    pub fn pop(self) -> System::Object { self.virt0::<"Pop", System::Object>() }
    pub fn push(self, a1: System::Object) { self.instance1::<"Push", System::Object, ()>(a1) }
    pub fn synchronized(a1: System::Collections::Stack) -> System::Collections::Stack { Self::static1::<"Synchronized", System::Collections::Stack, System::Collections::Stack>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
}
pub mod Linq{
pub mod Expressions{
pub mod Interpreter{
pub type LightLambda =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.Interpreter.LightLambda">;
use super::super::super::super::*;
impl From<LightLambda> for System::Object {
 fn from(v:LightLambda)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,LightLambda>(v)
}} 
}
pub type BinaryExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.BinaryExpression">;
use super::super::super::*;
impl From<BinaryExpression> for System::Linq::Expressions::Expression {
 fn from(v:BinaryExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,BinaryExpression>(v)
}} 
impl BinaryExpression {
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn get_right(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Right", System::Linq::Expressions::Expression>() }
    pub fn get_left(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Left", System::Linq::Expressions::Expression>() }
    pub fn get_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Method", System::Reflection::MethodInfo>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
    pub fn get_conversion(self) -> System::Linq::Expressions::LambdaExpression { self.instance0::<"get_Conversion", System::Linq::Expressions::LambdaExpression>() }
    pub fn get_is_lifted(self) -> bool { self.instance0::<"get_IsLifted", bool>() }
    pub fn get_is_lifted_to_null(self) -> bool { self.instance0::<"get_IsLiftedToNull", bool>() }
}
pub type Expression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.Expression">;
use super::super::super::*;
impl From<Expression> for System::Object {
 fn from(v:Expression)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Expression>(v)
}} 
impl Expression {
    pub fn assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Assign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Equal", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn reference_equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ReferenceEqual", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn not_equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"NotEqual", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn reference_not_equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ReferenceNotEqual", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn greater_than(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"GreaterThan", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn less_than(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"LessThan", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn greater_than_or_equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"GreaterThanOrEqual", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn less_than_or_equal(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"LessThanOrEqual", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn and_also(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"AndAlso", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn or_else(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"OrElse", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn coalesce(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Coalesce", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn add(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Add", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn add_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"AddAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn add_assign_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"AddAssignChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn add_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"AddChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn subtract(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Subtract", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn subtract_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"SubtractAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn subtract_assign_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"SubtractAssignChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn subtract_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"SubtractChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn divide(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Divide", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn divide_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"DivideAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn modulo(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Modulo", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn modulo_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ModuloAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn multiply(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Multiply", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn multiply_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"MultiplyAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn multiply_assign_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"MultiplyAssignChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn multiply_checked(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"MultiplyChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn left_shift(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"LeftShift", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn left_shift_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"LeftShiftAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn right_shift(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"RightShift", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn right_shift_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"RightShiftAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn and(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"And", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn and_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"AndAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn or(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Or", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn or_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"OrAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn exclusive_or(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ExclusiveOr", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn exclusive_or_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ExclusiveOrAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn power(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"Power", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn power_assign(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"PowerAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn array_index(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BinaryExpression { Self::static2::<"ArrayIndex", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BinaryExpression>(a1, a2) }
    pub fn block(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::BlockExpression { Self::static2::<"Block", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::BlockExpression>(a1, a2) }
    pub fn catch(a1: System::Type, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::CatchBlock { Self::static2::<"Catch", System::Type, System::Linq::Expressions::Expression, System::Linq::Expressions::CatchBlock>(a1, a2) }
    pub fn if_then(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::ConditionalExpression { Self::static2::<"IfThen", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::ConditionalExpression>(a1, a2) }
    pub fn constant(a1: System::Object) -> System::Linq::Expressions::ConstantExpression { Self::static1::<"Constant", System::Object, System::Linq::Expressions::ConstantExpression>(a1) }
    pub fn clear_debug_info(a1: System::Linq::Expressions::SymbolDocumentInfo) -> System::Linq::Expressions::DebugInfoExpression { Self::static1::<"ClearDebugInfo", System::Linq::Expressions::SymbolDocumentInfo, System::Linq::Expressions::DebugInfoExpression>(a1) }
    pub fn empty() -> System::Linq::Expressions::DefaultExpression { Self::static0::<"Empty", System::Linq::Expressions::DefaultExpression>() }
    pub fn default(a1: System::Type) -> System::Linq::Expressions::DefaultExpression { Self::static1::<"Default", System::Type, System::Linq::Expressions::DefaultExpression>(a1) }
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
    pub fn reduce_and_check(self) -> System::Linq::Expressions::Expression { self.instance0::<"ReduceAndCheck", System::Linq::Expressions::Expression>() }
    pub fn reduce_extensions(self) -> System::Linq::Expressions::Expression { self.instance0::<"ReduceExtensions", System::Linq::Expressions::Expression>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn r#break(a1: System::Linq::Expressions::LabelTarget) -> System::Linq::Expressions::GotoExpression { Self::static1::<"Break", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::GotoExpression>(a1) }
    pub fn r#continue(a1: System::Linq::Expressions::LabelTarget) -> System::Linq::Expressions::GotoExpression { Self::static1::<"Continue", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::GotoExpression>(a1) }
    pub fn r#return(a1: System::Linq::Expressions::LabelTarget) -> System::Linq::Expressions::GotoExpression { Self::static1::<"Return", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::GotoExpression>(a1) }
    pub fn goto(a1: System::Linq::Expressions::LabelTarget) -> System::Linq::Expressions::GotoExpression { Self::static1::<"Goto", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::GotoExpression>(a1) }
    pub fn label(a1: System::Linq::Expressions::LabelTarget) -> System::Linq::Expressions::LabelExpression { Self::static1::<"Label", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::LabelExpression>(a1) }
    pub fn r#loop(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::LoopExpression { Self::static1::<"Loop", System::Linq::Expressions::Expression, System::Linq::Expressions::LoopExpression>(a1) }
    pub fn bind(a1: System::Reflection::MemberInfo, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::MemberAssignment { Self::static2::<"Bind", System::Reflection::MemberInfo, System::Linq::Expressions::Expression, System::Linq::Expressions::MemberAssignment>(a1, a2) }
    pub fn field(a1: System::Linq::Expressions::Expression, a2: System::Reflection::FieldInfo) -> System::Linq::Expressions::MemberExpression { Self::static2::<"Field", System::Linq::Expressions::Expression, System::Reflection::FieldInfo, System::Linq::Expressions::MemberExpression>(a1, a2) }
    pub fn property(a1: System::Linq::Expressions::Expression, a2: System::String) -> System::Linq::Expressions::MemberExpression { Self::static2::<"Property", System::Linq::Expressions::Expression, System::String, System::Linq::Expressions::MemberExpression>(a1, a2) }
    pub fn property_or_field(a1: System::Linq::Expressions::Expression, a2: System::String) -> System::Linq::Expressions::MemberExpression { Self::static2::<"PropertyOrField", System::Linq::Expressions::Expression, System::String, System::Linq::Expressions::MemberExpression>(a1, a2) }
    pub fn make_member_access(a1: System::Linq::Expressions::Expression, a2: System::Reflection::MemberInfo) -> System::Linq::Expressions::MemberExpression { Self::static2::<"MakeMemberAccess", System::Linq::Expressions::Expression, System::Reflection::MemberInfo, System::Linq::Expressions::MemberExpression>(a1, a2) }
    pub fn call(a1: System::Reflection::MethodInfo, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::MethodCallExpression { Self::static2::<"Call", System::Reflection::MethodInfo, System::Linq::Expressions::Expression, System::Linq::Expressions::MethodCallExpression>(a1, a2) }
    pub fn parameter(a1: System::Type) -> System::Linq::Expressions::ParameterExpression { Self::static1::<"Parameter", System::Type, System::Linq::Expressions::ParameterExpression>(a1) }
    pub fn variable(a1: System::Type) -> System::Linq::Expressions::ParameterExpression { Self::static1::<"Variable", System::Type, System::Linq::Expressions::ParameterExpression>(a1) }
    pub fn symbol_document(a1: System::String) -> System::Linq::Expressions::SymbolDocumentInfo { Self::static1::<"SymbolDocument", System::String, System::Linq::Expressions::SymbolDocumentInfo>(a1) }
    pub fn try_fault(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::TryExpression { Self::static2::<"TryFault", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::TryExpression>(a1, a2) }
    pub fn try_finally(a1: System::Linq::Expressions::Expression, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::TryExpression { Self::static2::<"TryFinally", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression, System::Linq::Expressions::TryExpression>(a1, a2) }
    pub fn type_is(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::TypeBinaryExpression { Self::static2::<"TypeIs", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::TypeBinaryExpression>(a1, a2) }
    pub fn type_equal(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::TypeBinaryExpression { Self::static2::<"TypeEqual", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::TypeBinaryExpression>(a1, a2) }
    pub fn negate(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Negate", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn unary_plus(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"UnaryPlus", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn negate_checked(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"NegateChecked", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn not(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Not", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn is_false(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"IsFalse", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn is_true(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"IsTrue", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn ones_complement(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"OnesComplement", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn type_as(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::UnaryExpression { Self::static2::<"TypeAs", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::UnaryExpression>(a1, a2) }
    pub fn unbox(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::UnaryExpression { Self::static2::<"Unbox", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::UnaryExpression>(a1, a2) }
    pub fn convert(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::UnaryExpression { Self::static2::<"Convert", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::UnaryExpression>(a1, a2) }
    pub fn convert_checked(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Linq::Expressions::UnaryExpression { Self::static2::<"ConvertChecked", System::Linq::Expressions::Expression, System::Type, System::Linq::Expressions::UnaryExpression>(a1, a2) }
    pub fn array_length(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"ArrayLength", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn quote(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Quote", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn rethrow() -> System::Linq::Expressions::UnaryExpression { Self::static0::<"Rethrow", System::Linq::Expressions::UnaryExpression>() }
    pub fn throw(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Throw", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn increment(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Increment", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn decrement(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"Decrement", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn pre_increment_assign(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"PreIncrementAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn pre_decrement_assign(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"PreDecrementAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn post_increment_assign(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"PostIncrementAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
    pub fn post_decrement_assign(a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { Self::static1::<"PostDecrementAssign", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
}
pub type BlockExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.BlockExpression">;
use super::super::super::*;
impl From<BlockExpression> for System::Linq::Expressions::Expression {
 fn from(v:BlockExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,BlockExpression>(v)
}} 
impl BlockExpression {
    pub fn get_result(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Result", System::Linq::Expressions::Expression>() }
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
}
pub type CatchBlock =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.CatchBlock">;
use super::super::super::*;
impl From<CatchBlock> for System::Object {
 fn from(v:CatchBlock)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CatchBlock>(v)
}} 
impl CatchBlock {
    pub fn get_variable(self) -> System::Linq::Expressions::ParameterExpression { self.instance0::<"get_Variable", System::Linq::Expressions::ParameterExpression>() }
    pub fn get_test(self) -> System::Type { self.instance0::<"get_Test", System::Type>() }
    pub fn get_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Body", System::Linq::Expressions::Expression>() }
    pub fn get_filter(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Filter", System::Linq::Expressions::Expression>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ConditionalExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ConditionalExpression">;
use super::super::super::*;
impl From<ConditionalExpression> for System::Linq::Expressions::Expression {
 fn from(v:ConditionalExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,ConditionalExpression>(v)
}} 
impl ConditionalExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_test(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Test", System::Linq::Expressions::Expression>() }
    pub fn get_if_true(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_IfTrue", System::Linq::Expressions::Expression>() }
    pub fn get_if_false(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_IfFalse", System::Linq::Expressions::Expression>() }
}
pub type ConstantExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ConstantExpression">;
use super::super::super::*;
impl From<ConstantExpression> for System::Linq::Expressions::Expression {
 fn from(v:ConstantExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,ConstantExpression>(v)
}} 
impl ConstantExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_value(self) -> System::Object { self.instance0::<"get_Value", System::Object>() }
}
pub type DebugInfoExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.DebugInfoExpression">;
use super::super::super::*;
impl From<DebugInfoExpression> for System::Linq::Expressions::Expression {
 fn from(v:DebugInfoExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,DebugInfoExpression>(v)
}} 
impl DebugInfoExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_start_line(self) -> i32 { self.virt0::<"get_StartLine", i32>() }
    pub fn get_start_column(self) -> i32 { self.virt0::<"get_StartColumn", i32>() }
    pub fn get_end_line(self) -> i32 { self.virt0::<"get_EndLine", i32>() }
    pub fn get_end_column(self) -> i32 { self.virt0::<"get_EndColumn", i32>() }
    pub fn get_document(self) -> System::Linq::Expressions::SymbolDocumentInfo { self.instance0::<"get_Document", System::Linq::Expressions::SymbolDocumentInfo>() }
    pub fn get_is_clear(self) -> bool { self.virt0::<"get_IsClear", bool>() }
}
pub type DefaultExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.DefaultExpression">;
use super::super::super::*;
impl From<DefaultExpression> for System::Linq::Expressions::Expression {
 fn from(v:DefaultExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,DefaultExpression>(v)
}} 
impl DefaultExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
}
pub type ElementInit =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ElementInit">;
use super::super::super::*;
impl From<ElementInit> for System::Object {
 fn from(v:ElementInit)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ElementInit>(v)
}} 
impl ElementInit {
    pub fn get_add_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_AddMethod", System::Reflection::MethodInfo>() }
    pub fn get_argument(self, a1: i32) -> System::Linq::Expressions::Expression { self.instance1::<"GetArgument", i32, System::Linq::Expressions::Expression>(a1) }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type ExpressionVisitor =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ExpressionVisitor">;
use super::super::super::*;
impl From<ExpressionVisitor> for System::Object {
 fn from(v:ExpressionVisitor)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExpressionVisitor>(v)
}} 
impl ExpressionVisitor {
    pub fn visit(self, a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::Expression { self.instance1::<"Visit", System::Linq::Expressions::Expression, System::Linq::Expressions::Expression>(a1) }
}
pub type GotoExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.GotoExpression">;
use super::super::super::*;
impl From<GotoExpression> for System::Linq::Expressions::Expression {
 fn from(v:GotoExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,GotoExpression>(v)
}} 
impl GotoExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_value(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Value", System::Linq::Expressions::Expression>() }
    pub fn get_target(self) -> System::Linq::Expressions::LabelTarget { self.instance0::<"get_Target", System::Linq::Expressions::LabelTarget>() }
    pub fn update(self, a1: System::Linq::Expressions::LabelTarget, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::GotoExpression { self.instance2::<"Update", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::Expression, System::Linq::Expressions::GotoExpression>(a1, a2) }
}
pub type IArgumentProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.IArgumentProvider">;
use super::super::super::*;
impl IArgumentProvider {
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
}
pub type IDynamicExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.IDynamicExpression">;
use super::super::super::*;
impl IDynamicExpression {
    pub fn get_delegate_type(self) -> System::Type { self.virt0::<"get_DelegateType", System::Type>() }
    pub fn create_call_site(self) -> System::Object { self.virt0::<"CreateCallSite", System::Object>() }
}
pub type IndexExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.IndexExpression">;
use super::super::super::*;
impl From<IndexExpression> for System::Linq::Expressions::Expression {
 fn from(v:IndexExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,IndexExpression>(v)
}} 
impl IndexExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_object(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Object", System::Linq::Expressions::Expression>() }
    pub fn get_indexer(self) -> System::Reflection::PropertyInfo { self.instance0::<"get_Indexer", System::Reflection::PropertyInfo>() }
    pub fn get_argument(self, a1: i32) -> System::Linq::Expressions::Expression { self.instance1::<"GetArgument", i32, System::Linq::Expressions::Expression>(a1) }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
}
pub type InvocationExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.InvocationExpression">;
use super::super::super::*;
impl From<InvocationExpression> for System::Linq::Expressions::Expression {
 fn from(v:InvocationExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,InvocationExpression>(v)
}} 
impl InvocationExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn get_argument(self, a1: i32) -> System::Linq::Expressions::Expression { self.instance1::<"GetArgument", i32, System::Linq::Expressions::Expression>(a1) }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
}
pub type LabelExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.LabelExpression">;
use super::super::super::*;
impl From<LabelExpression> for System::Linq::Expressions::Expression {
 fn from(v:LabelExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,LabelExpression>(v)
}} 
impl LabelExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_target(self) -> System::Linq::Expressions::LabelTarget { self.instance0::<"get_Target", System::Linq::Expressions::LabelTarget>() }
    pub fn get_default_value(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_DefaultValue", System::Linq::Expressions::Expression>() }
    pub fn update(self, a1: System::Linq::Expressions::LabelTarget, a2: System::Linq::Expressions::Expression) -> System::Linq::Expressions::LabelExpression { self.instance2::<"Update", System::Linq::Expressions::LabelTarget, System::Linq::Expressions::Expression, System::Linq::Expressions::LabelExpression>(a1, a2) }
}
pub type LabelTarget =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.LabelTarget">;
use super::super::super::*;
impl From<LabelTarget> for System::Object {
 fn from(v:LabelTarget)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,LabelTarget>(v)
}} 
impl LabelTarget {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_type(self) -> System::Type { self.instance0::<"get_Type", System::Type>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type LambdaExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.LambdaExpression">;
use super::super::super::*;
impl From<LambdaExpression> for System::Linq::Expressions::Expression {
 fn from(v:LambdaExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,LambdaExpression>(v)
}} 
impl LambdaExpression {
    pub fn get_can_compile_to_il() -> bool { Self::static0::<"get_CanCompileToIL", bool>() }
    pub fn get_can_interpret() -> bool { Self::static0::<"get_CanInterpret", bool>() }
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Body", System::Linq::Expressions::Expression>() }
    pub fn get_return_type(self) -> System::Type { self.instance0::<"get_ReturnType", System::Type>() }
    pub fn get_tail_call(self) -> bool { self.instance0::<"get_TailCall", bool>() }
    pub fn compile(self) -> System::Delegate { self.instance0::<"Compile", System::Delegate>() }
}
pub type ListInitExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ListInitExpression">;
use super::super::super::*;
impl From<ListInitExpression> for System::Linq::Expressions::Expression {
 fn from(v:ListInitExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,ListInitExpression>(v)
}} 
impl ListInitExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn get_new_expression(self) -> System::Linq::Expressions::NewExpression { self.instance0::<"get_NewExpression", System::Linq::Expressions::NewExpression>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
}
pub type LoopExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.LoopExpression">;
use super::super::super::*;
impl From<LoopExpression> for System::Linq::Expressions::Expression {
 fn from(v:LoopExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,LoopExpression>(v)
}} 
impl LoopExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Body", System::Linq::Expressions::Expression>() }
    pub fn get_break_label(self) -> System::Linq::Expressions::LabelTarget { self.instance0::<"get_BreakLabel", System::Linq::Expressions::LabelTarget>() }
    pub fn get_continue_label(self) -> System::Linq::Expressions::LabelTarget { self.instance0::<"get_ContinueLabel", System::Linq::Expressions::LabelTarget>() }
}
pub type MemberAssignment =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberAssignment">;
use super::super::super::*;
impl From<MemberAssignment> for System::Linq::Expressions::MemberBinding {
 fn from(v:MemberAssignment)->System::Linq::Expressions::MemberBinding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::MemberBinding,MemberAssignment>(v)
}} 
impl MemberAssignment {
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn update(self, a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::MemberAssignment { self.instance1::<"Update", System::Linq::Expressions::Expression, System::Linq::Expressions::MemberAssignment>(a1) }
}
pub type MemberBinding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberBinding">;
use super::super::super::*;
impl From<MemberBinding> for System::Object {
 fn from(v:MemberBinding)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MemberBinding>(v)
}} 
impl MemberBinding {
    pub fn get_member(self) -> System::Reflection::MemberInfo { self.instance0::<"get_Member", System::Reflection::MemberInfo>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type MemberExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberExpression">;
use super::super::super::*;
impl From<MemberExpression> for System::Linq::Expressions::Expression {
 fn from(v:MemberExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,MemberExpression>(v)
}} 
impl MemberExpression {
    pub fn get_member(self) -> System::Reflection::MemberInfo { self.instance0::<"get_Member", System::Reflection::MemberInfo>() }
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn update(self, a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::MemberExpression { self.instance1::<"Update", System::Linq::Expressions::Expression, System::Linq::Expressions::MemberExpression>(a1) }
}
pub type MemberInitExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberInitExpression">;
use super::super::super::*;
impl From<MemberInitExpression> for System::Linq::Expressions::Expression {
 fn from(v:MemberInitExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,MemberInitExpression>(v)
}} 
impl MemberInitExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn get_new_expression(self) -> System::Linq::Expressions::NewExpression { self.instance0::<"get_NewExpression", System::Linq::Expressions::NewExpression>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
}
pub type MemberListBinding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberListBinding">;
use super::super::super::*;
impl From<MemberListBinding> for System::Linq::Expressions::MemberBinding {
 fn from(v:MemberListBinding)->System::Linq::Expressions::MemberBinding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::MemberBinding,MemberListBinding>(v)
}} 
pub type MemberMemberBinding =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MemberMemberBinding">;
use super::super::super::*;
impl From<MemberMemberBinding> for System::Linq::Expressions::MemberBinding {
 fn from(v:MemberMemberBinding)->System::Linq::Expressions::MemberBinding{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::MemberBinding,MemberMemberBinding>(v)
}} 
pub type MethodCallExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.MethodCallExpression">;
use super::super::super::*;
impl From<MethodCallExpression> for System::Linq::Expressions::Expression {
 fn from(v:MethodCallExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,MethodCallExpression>(v)
}} 
impl MethodCallExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Method", System::Reflection::MethodInfo>() }
    pub fn get_object(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Object", System::Linq::Expressions::Expression>() }
    pub fn get_argument(self, a1: i32) -> System::Linq::Expressions::Expression { self.instance1::<"GetArgument", i32, System::Linq::Expressions::Expression>(a1) }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
}
pub type NewArrayExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.NewArrayExpression">;
use super::super::super::*;
impl From<NewArrayExpression> for System::Linq::Expressions::Expression {
 fn from(v:NewArrayExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,NewArrayExpression>(v)
}} 
impl NewArrayExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
}
pub type NewExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.NewExpression">;
use super::super::super::*;
impl From<NewExpression> for System::Linq::Expressions::Expression {
 fn from(v:NewExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,NewExpression>(v)
}} 
impl NewExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_constructor(self) -> System::Reflection::ConstructorInfo { self.instance0::<"get_Constructor", System::Reflection::ConstructorInfo>() }
    pub fn get_argument(self, a1: i32) -> System::Linq::Expressions::Expression { self.instance1::<"GetArgument", i32, System::Linq::Expressions::Expression>(a1) }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
}
pub type ParameterExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.ParameterExpression">;
use super::super::super::*;
impl From<ParameterExpression> for System::Linq::Expressions::Expression {
 fn from(v:ParameterExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,ParameterExpression>(v)
}} 
impl ParameterExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_is_by_ref(self) -> bool { self.instance0::<"get_IsByRef", bool>() }
}
pub type RuntimeVariablesExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.RuntimeVariablesExpression">;
use super::super::super::*;
impl From<RuntimeVariablesExpression> for System::Linq::Expressions::Expression {
 fn from(v:RuntimeVariablesExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,RuntimeVariablesExpression>(v)
}} 
impl RuntimeVariablesExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
}
pub type SwitchCase =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.SwitchCase">;
use super::super::super::*;
impl From<SwitchCase> for System::Object {
 fn from(v:SwitchCase)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SwitchCase>(v)
}} 
impl SwitchCase {
    pub fn get_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Body", System::Linq::Expressions::Expression>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type SwitchExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.SwitchExpression">;
use super::super::super::*;
impl From<SwitchExpression> for System::Linq::Expressions::Expression {
 fn from(v:SwitchExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,SwitchExpression>(v)
}} 
impl SwitchExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_switch_value(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_SwitchValue", System::Linq::Expressions::Expression>() }
    pub fn get_default_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_DefaultBody", System::Linq::Expressions::Expression>() }
    pub fn get_comparison(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Comparison", System::Reflection::MethodInfo>() }
}
pub type SymbolDocumentInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.SymbolDocumentInfo">;
use super::super::super::*;
impl From<SymbolDocumentInfo> for System::Object {
 fn from(v:SymbolDocumentInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,SymbolDocumentInfo>(v)
}} 
impl SymbolDocumentInfo {
    pub fn get_file_name(self) -> System::String { self.instance0::<"get_FileName", System::String>() }
}
pub type TryExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.TryExpression">;
use super::super::super::*;
impl From<TryExpression> for System::Linq::Expressions::Expression {
 fn from(v:TryExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,TryExpression>(v)
}} 
impl TryExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_body(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Body", System::Linq::Expressions::Expression>() }
    pub fn get_finally(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Finally", System::Linq::Expressions::Expression>() }
    pub fn get_fault(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Fault", System::Linq::Expressions::Expression>() }
}
pub type TypeBinaryExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.TypeBinaryExpression">;
use super::super::super::*;
impl From<TypeBinaryExpression> for System::Linq::Expressions::Expression {
 fn from(v:TypeBinaryExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,TypeBinaryExpression>(v)
}} 
impl TypeBinaryExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn get_type_operand(self) -> System::Type { self.instance0::<"get_TypeOperand", System::Type>() }
    pub fn update(self, a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::TypeBinaryExpression { self.instance1::<"Update", System::Linq::Expressions::Expression, System::Linq::Expressions::TypeBinaryExpression>(a1) }
}
pub type UnaryExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.UnaryExpression">;
use super::super::super::*;
impl From<UnaryExpression> for System::Linq::Expressions::Expression {
 fn from(v:UnaryExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,UnaryExpression>(v)
}} 
impl UnaryExpression {
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_operand(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Operand", System::Linq::Expressions::Expression>() }
    pub fn get_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Method", System::Reflection::MethodInfo>() }
    pub fn get_is_lifted(self) -> bool { self.instance0::<"get_IsLifted", bool>() }
    pub fn get_is_lifted_to_null(self) -> bool { self.instance0::<"get_IsLiftedToNull", bool>() }
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
    pub fn update(self, a1: System::Linq::Expressions::Expression) -> System::Linq::Expressions::UnaryExpression { self.instance1::<"Update", System::Linq::Expressions::Expression, System::Linq::Expressions::UnaryExpression>(a1) }
}
pub type DynamicExpressionVisitor =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.DynamicExpressionVisitor">;
use super::super::super::*;
impl From<DynamicExpressionVisitor> for System::Linq::Expressions::ExpressionVisitor {
 fn from(v:DynamicExpressionVisitor)->System::Linq::Expressions::ExpressionVisitor{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::ExpressionVisitor,DynamicExpressionVisitor>(v)
}} 
impl DynamicExpressionVisitor {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DynamicExpression =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.Expressions.DynamicExpression">;
use super::super::super::*;
impl From<DynamicExpression> for System::Linq::Expressions::Expression {
 fn from(v:DynamicExpression)->System::Linq::Expressions::Expression{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Linq::Expressions::Expression,DynamicExpression>(v)
}} 
impl DynamicExpression {
    pub fn get_can_reduce(self) -> bool { self.virt0::<"get_CanReduce", bool>() }
    pub fn reduce(self) -> System::Linq::Expressions::Expression { self.virt0::<"Reduce", System::Linq::Expressions::Expression>() }
    pub fn get_type(self) -> System::Type { self.virt0::<"get_Type", System::Type>() }
    pub fn get_binder(self) -> System::Runtime::CompilerServices::CallSiteBinder { self.instance0::<"get_Binder", System::Runtime::CompilerServices::CallSiteBinder>() }
    pub fn get_delegate_type(self) -> System::Type { self.virt0::<"get_DelegateType", System::Type>() }
}
}
pub type Enumerable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq","System.Linq.Enumerable">;
use super::super::*;
impl From<Enumerable> for System::Object {
 fn from(v:Enumerable)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Enumerable>(v)
}} 
pub type IQueryable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.IQueryable">;
use super::super::*;
impl IQueryable {
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.virt0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn get_element_type(self) -> System::Type { self.virt0::<"get_ElementType", System::Type>() }
    pub fn get_provider(self) -> System::Linq::IQueryProvider { self.virt0::<"get_Provider", System::Linq::IQueryProvider>() }
}
pub type IQueryProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.IQueryProvider">;
use super::super::*;
pub type IOrderedQueryable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Linq.IOrderedQueryable">;
use super::super::*;
}
pub mod Dynamic{
pub type DynamicMetaObjectBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.DynamicMetaObjectBinder">;
use super::super::*;
impl From<DynamicMetaObjectBinder> for System::Runtime::CompilerServices::CallSiteBinder {
 fn from(v:DynamicMetaObjectBinder)->System::Runtime::CompilerServices::CallSiteBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Runtime::CompilerServices::CallSiteBinder,DynamicMetaObjectBinder>(v)
}} 
impl DynamicMetaObjectBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_update_expression(self, a1: System::Type) -> System::Linq::Expressions::Expression { self.instance1::<"GetUpdateExpression", System::Type, System::Linq::Expressions::Expression>(a1) }
}
pub type DynamicMetaObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.DynamicMetaObject">;
use super::super::*;
impl From<DynamicMetaObject> for System::Object {
 fn from(v:DynamicMetaObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DynamicMetaObject>(v)
}} 
impl DynamicMetaObject {
    pub fn get_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"get_Expression", System::Linq::Expressions::Expression>() }
    pub fn get_restrictions(self) -> System::Dynamic::BindingRestrictions { self.instance0::<"get_Restrictions", System::Dynamic::BindingRestrictions>() }
    pub fn get_value(self) -> System::Object { self.instance0::<"get_Value", System::Object>() }
    pub fn get_has_value(self) -> bool { self.instance0::<"get_HasValue", bool>() }
    pub fn get_runtime_type(self) -> System::Type { self.instance0::<"get_RuntimeType", System::Type>() }
    pub fn get_limit_type(self) -> System::Type { self.instance0::<"get_LimitType", System::Type>() }
    pub fn bind_convert(self, a1: System::Dynamic::ConvertBinder) -> System::Dynamic::DynamicMetaObject { self.instance1::<"BindConvert", System::Dynamic::ConvertBinder, System::Dynamic::DynamicMetaObject>(a1) }
    pub fn bind_get_member(self, a1: System::Dynamic::GetMemberBinder) -> System::Dynamic::DynamicMetaObject { self.instance1::<"BindGetMember", System::Dynamic::GetMemberBinder, System::Dynamic::DynamicMetaObject>(a1) }
    pub fn bind_set_member(self, a1: System::Dynamic::SetMemberBinder, a2: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance2::<"BindSetMember", System::Dynamic::SetMemberBinder, System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1, a2) }
    pub fn bind_delete_member(self, a1: System::Dynamic::DeleteMemberBinder) -> System::Dynamic::DynamicMetaObject { self.instance1::<"BindDeleteMember", System::Dynamic::DeleteMemberBinder, System::Dynamic::DynamicMetaObject>(a1) }
    pub fn bind_unary_operation(self, a1: System::Dynamic::UnaryOperationBinder) -> System::Dynamic::DynamicMetaObject { self.instance1::<"BindUnaryOperation", System::Dynamic::UnaryOperationBinder, System::Dynamic::DynamicMetaObject>(a1) }
    pub fn bind_binary_operation(self, a1: System::Dynamic::BinaryOperationBinder, a2: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance2::<"BindBinaryOperation", System::Dynamic::BinaryOperationBinder, System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1, a2) }
    pub fn create(a1: System::Object, a2: System::Linq::Expressions::Expression) -> System::Dynamic::DynamicMetaObject { Self::static2::<"Create", System::Object, System::Linq::Expressions::Expression, System::Dynamic::DynamicMetaObject>(a1, a2) }
    pub fn new(a1: System::Linq::Expressions::Expression, a2: System::Dynamic::BindingRestrictions) -> Self { Self::ctor2(a1, a2) }
}
pub type IDynamicMetaObjectProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.IDynamicMetaObjectProvider">;
use super::super::*;
pub type BindingRestrictions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.BindingRestrictions">;
use super::super::*;
impl From<BindingRestrictions> for System::Object {
 fn from(v:BindingRestrictions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BindingRestrictions>(v)
}} 
impl BindingRestrictions {
    pub fn merge(self, a1: System::Dynamic::BindingRestrictions) -> System::Dynamic::BindingRestrictions { self.instance1::<"Merge", System::Dynamic::BindingRestrictions, System::Dynamic::BindingRestrictions>(a1) }
    pub fn get_type_restriction(a1: System::Linq::Expressions::Expression, a2: System::Type) -> System::Dynamic::BindingRestrictions { Self::static2::<"GetTypeRestriction", System::Linq::Expressions::Expression, System::Type, System::Dynamic::BindingRestrictions>(a1, a2) }
    pub fn get_instance_restriction(a1: System::Linq::Expressions::Expression, a2: System::Object) -> System::Dynamic::BindingRestrictions { Self::static2::<"GetInstanceRestriction", System::Linq::Expressions::Expression, System::Object, System::Dynamic::BindingRestrictions>(a1, a2) }
    pub fn get_expression_restriction(a1: System::Linq::Expressions::Expression) -> System::Dynamic::BindingRestrictions { Self::static1::<"GetExpressionRestriction", System::Linq::Expressions::Expression, System::Dynamic::BindingRestrictions>(a1) }
    pub fn to_expression(self) -> System::Linq::Expressions::Expression { self.instance0::<"ToExpression", System::Linq::Expressions::Expression>() }
}
pub type BinaryOperationBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.BinaryOperationBinder">;
use super::super::*;
impl From<BinaryOperationBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:BinaryOperationBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,BinaryOperationBinder>(v)
}} 
impl BinaryOperationBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn fallback_binary_operation(self, a1: System::Dynamic::DynamicMetaObject, a2: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance2::<"FallbackBinaryOperation", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1, a2) }
}
pub type CallInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.CallInfo">;
use super::super::*;
impl From<CallInfo> for System::Object {
 fn from(v:CallInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CallInfo>(v)
}} 
impl CallInfo {
    pub fn get_argument_count(self) -> i32 { self.instance0::<"get_ArgumentCount", i32>() }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
}
pub type ExpandoObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.ExpandoObject">;
use super::super::*;
impl From<ExpandoObject> for System::Object {
 fn from(v:ExpandoObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ExpandoObject>(v)
}} 
impl ExpandoObject {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ConvertBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.ConvertBinder">;
use super::super::*;
impl From<ConvertBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:ConvertBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,ConvertBinder>(v)
}} 
impl ConvertBinder {
    pub fn get_type(self) -> System::Type { self.instance0::<"get_Type", System::Type>() }
    pub fn get_explicit(self) -> bool { self.instance0::<"get_Explicit", bool>() }
    pub fn fallback_convert(self, a1: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance1::<"FallbackConvert", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1) }
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
}
pub type CreateInstanceBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.CreateInstanceBinder">;
use super::super::*;
impl From<CreateInstanceBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:CreateInstanceBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,CreateInstanceBinder>(v)
}} 
impl CreateInstanceBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type DeleteIndexBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.DeleteIndexBinder">;
use super::super::*;
impl From<DeleteIndexBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:DeleteIndexBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,DeleteIndexBinder>(v)
}} 
impl DeleteIndexBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type DeleteMemberBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.DeleteMemberBinder">;
use super::super::*;
impl From<DeleteMemberBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:DeleteMemberBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,DeleteMemberBinder>(v)
}} 
impl DeleteMemberBinder {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_ignore_case(self) -> bool { self.instance0::<"get_IgnoreCase", bool>() }
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn fallback_delete_member(self, a1: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance1::<"FallbackDeleteMember", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1) }
}
pub type DynamicObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.DynamicObject">;
use super::super::*;
impl From<DynamicObject> for System::Object {
 fn from(v:DynamicObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DynamicObject>(v)
}} 
impl DynamicObject {
    pub fn try_set_member(self, a1: System::Dynamic::SetMemberBinder, a2: System::Object) -> bool { self.instance2::<"TrySetMember", System::Dynamic::SetMemberBinder, System::Object, bool>(a1, a2) }
    pub fn try_delete_member(self, a1: System::Dynamic::DeleteMemberBinder) -> bool { self.instance1::<"TryDeleteMember", System::Dynamic::DeleteMemberBinder, bool>(a1) }
    pub fn get_meta_object(self, a1: System::Linq::Expressions::Expression) -> System::Dynamic::DynamicMetaObject { self.instance1::<"GetMetaObject", System::Linq::Expressions::Expression, System::Dynamic::DynamicMetaObject>(a1) }
}
pub type GetIndexBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.GetIndexBinder">;
use super::super::*;
impl From<GetIndexBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:GetIndexBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,GetIndexBinder>(v)
}} 
impl GetIndexBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type GetMemberBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.GetMemberBinder">;
use super::super::*;
impl From<GetMemberBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:GetMemberBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,GetMemberBinder>(v)
}} 
impl GetMemberBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_ignore_case(self) -> bool { self.instance0::<"get_IgnoreCase", bool>() }
    pub fn fallback_get_member(self, a1: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance1::<"FallbackGetMember", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1) }
}
pub type InvokeBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.InvokeBinder">;
use super::super::*;
impl From<InvokeBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:InvokeBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,InvokeBinder>(v)
}} 
impl InvokeBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type InvokeMemberBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.InvokeMemberBinder">;
use super::super::*;
impl From<InvokeMemberBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:InvokeMemberBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,InvokeMemberBinder>(v)
}} 
impl InvokeMemberBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_ignore_case(self) -> bool { self.instance0::<"get_IgnoreCase", bool>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type SetIndexBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.SetIndexBinder">;
use super::super::*;
impl From<SetIndexBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:SetIndexBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,SetIndexBinder>(v)
}} 
impl SetIndexBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_call_info(self) -> System::Dynamic::CallInfo { self.instance0::<"get_CallInfo", System::Dynamic::CallInfo>() }
}
pub type SetMemberBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.SetMemberBinder">;
use super::super::*;
impl From<SetMemberBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:SetMemberBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,SetMemberBinder>(v)
}} 
impl SetMemberBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_ignore_case(self) -> bool { self.instance0::<"get_IgnoreCase", bool>() }
    pub fn fallback_set_member(self, a1: System::Dynamic::DynamicMetaObject, a2: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance2::<"FallbackSetMember", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1, a2) }
}
pub type UnaryOperationBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.UnaryOperationBinder">;
use super::super::*;
impl From<UnaryOperationBinder> for System::Dynamic::DynamicMetaObjectBinder {
 fn from(v:UnaryOperationBinder)->System::Dynamic::DynamicMetaObjectBinder{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Dynamic::DynamicMetaObjectBinder,UnaryOperationBinder>(v)
}} 
impl UnaryOperationBinder {
    pub fn get_return_type(self) -> System::Type { self.virt0::<"get_ReturnType", System::Type>() }
    pub fn fallback_unary_operation(self, a1: System::Dynamic::DynamicMetaObject) -> System::Dynamic::DynamicMetaObject { self.instance1::<"FallbackUnaryOperation", System::Dynamic::DynamicMetaObject, System::Dynamic::DynamicMetaObject>(a1) }
}
pub type IInvokeOnGetBinder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Linq.Expressions","System.Dynamic.IInvokeOnGetBinder">;
use super::super::*;
impl IInvokeOnGetBinder {
    pub fn get_invoke_on_get(self) -> bool { self.virt0::<"get_InvokeOnGet", bool>() }
}
}
pub mod Windows{
pub mod Input{
pub type ICommand =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Windows.Input.ICommand">;
use super::super::super::*;
}
pub mod Markup{
pub type ValueSerializerAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ObjectModel","System.Windows.Markup.ValueSerializerAttribute">;
use super::super::super::*;
impl From<ValueSerializerAttribute> for System::Attribute {
 fn from(v:ValueSerializerAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ValueSerializerAttribute>(v)
}} 
impl ValueSerializerAttribute {
    pub fn get_value_serializer_type(self) -> System::Type { self.instance0::<"get_ValueSerializerType", System::Type>() }
    pub fn get_value_serializer_type_name(self) -> System::String { self.instance0::<"get_ValueSerializerTypeName", System::String>() }
    pub fn new(a1: System::Type) -> Self { Self::ctor1(a1) }
}
}
}
pub type Array =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Array">;
use super::*;
impl From<Array> for System::Object {
 fn from(v:Array)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Array>(v)
}} 
impl Array {
    pub fn clear(a1: System::Array) { Self::static1::<"Clear", System::Array, ()>(a1) }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn get_long_length(self) -> i64 { self.instance0::<"get_LongLength", i64>() }
    pub fn get_rank(self) -> i32 { self.instance0::<"get_Rank", i32>() }
    pub fn get_upper_bound(self, a1: i32) -> i32 { self.instance1::<"GetUpperBound", i32, i32>(a1) }
    pub fn get_lower_bound(self, a1: i32) -> i32 { self.instance1::<"GetLowerBound", i32, i32>(a1) }
    pub fn initialize(self) { self.instance0::<"Initialize", ()>() }
    pub fn create_instance(a1: System::Type, a2: i32) -> System::Array { Self::static2::<"CreateInstance", System::Type, i32, System::Array>(a1, a2) }
    pub fn get_value(self, a1: i32) -> System::Object { self.instance1::<"GetValue", i32, System::Object>(a1) }
    pub fn set_value(self, a1: System::Object, a2: i32) { self.instance2::<"SetValue", System::Object, i32, ()>(a1, a2) }
    pub fn get_sync_root(self) -> System::Object { self.virt0::<"get_SyncRoot", System::Object>() }
    pub fn get_is_read_only(self) -> bool { self.virt0::<"get_IsReadOnly", bool>() }
    pub fn get_is_fixed_size(self) -> bool { self.virt0::<"get_IsFixedSize", bool>() }
    pub fn get_is_synchronized(self) -> bool { self.virt0::<"get_IsSynchronized", bool>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn binary_search(a1: System::Array, a2: System::Object) -> i32 { Self::static2::<"BinarySearch", System::Array, System::Object, i32>(a1, a2) }
    pub fn copy_to(self, a1: System::Array, a2: i32) { self.instance2::<"CopyTo", System::Array, i32, ()>(a1, a2) }
    pub fn index_of(a1: System::Array, a2: System::Object) -> i32 { Self::static2::<"IndexOf", System::Array, System::Object, i32>(a1, a2) }
    pub fn last_index_of(a1: System::Array, a2: System::Object) -> i32 { Self::static2::<"LastIndexOf", System::Array, System::Object, i32>(a1, a2) }
    pub fn reverse(a1: System::Array) { Self::static1::<"Reverse", System::Array, ()>(a1) }
    pub fn sort(a1: System::Array) { Self::static1::<"Sort", System::Array, ()>(a1) }
    pub fn get_max_length() -> i32 { Self::static0::<"get_MaxLength", i32>() }
    pub fn get_enumerator(self) -> System::Collections::IEnumerator { self.virt0::<"GetEnumerator", System::Collections::IEnumerator>() }
}
pub type Attribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Attribute">;
use super::*;
impl From<Attribute> for System::Object {
 fn from(v:Attribute)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Attribute>(v)
}} 
impl Attribute {
    pub fn is_defined(a1: System::Reflection::MemberInfo, a2: System::Type) -> bool { Self::static2::<"IsDefined", System::Reflection::MemberInfo, System::Type, bool>(a1, a2) }
    pub fn get_custom_attribute(a1: System::Reflection::MemberInfo, a2: System::Type) -> System::Attribute { Self::static2::<"GetCustomAttribute", System::Reflection::MemberInfo, System::Type, System::Attribute>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn get_type_id(self) -> System::Object { self.virt0::<"get_TypeId", System::Object>() }
    pub fn r#match(self, a1: System::Object) -> bool { self.instance1::<"Match", System::Object, bool>(a1) }
    pub fn is_default_attribute(self) -> bool { self.virt0::<"IsDefaultAttribute", bool>() }
}
pub type BadImageFormatException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.BadImageFormatException">;
use super::*;
impl From<BadImageFormatException> for System::SystemException {
 fn from(v:BadImageFormatException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,BadImageFormatException>(v)
}} 
impl BadImageFormatException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_file_name(self) -> System::String { self.instance0::<"get_FileName", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_fusion_log(self) -> System::String { self.instance0::<"get_FusionLog", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type Buffer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Buffer">;
use super::*;
impl From<Buffer> for System::Object {
 fn from(v:Buffer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Buffer>(v)
}} 
impl Buffer {
    pub fn byte_length(a1: System::Array) -> i32 { Self::static1::<"ByteLength", System::Array, i32>(a1) }
    pub fn get_byte(a1: System::Array, a2: i32) -> u8 { Self::static2::<"GetByte", System::Array, i32, u8>(a1, a2) }
}
pub type Delegate =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Delegate">;
use super::*;
impl From<Delegate> for System::Object {
 fn from(v:Delegate)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Delegate>(v)
}} 
impl Delegate {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn get_target(self) -> System::Object { self.instance0::<"get_Target", System::Object>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn combine(a1: System::Delegate, a2: System::Delegate) -> System::Delegate { Self::static2::<"Combine", System::Delegate, System::Delegate, System::Delegate>(a1, a2) }
    pub fn create_delegate(a1: System::Type, a2: System::Reflection::MethodInfo) -> System::Delegate { Self::static2::<"CreateDelegate", System::Type, System::Reflection::MethodInfo, System::Delegate>(a1, a2) }
    pub fn get_method(self) -> System::Reflection::MethodInfo { self.instance0::<"get_Method", System::Reflection::MethodInfo>() }
    pub fn remove(a1: System::Delegate, a2: System::Delegate) -> System::Delegate { Self::static2::<"Remove", System::Delegate, System::Delegate, System::Delegate>(a1, a2) }
    pub fn remove_all(a1: System::Delegate, a2: System::Delegate) -> System::Delegate { Self::static2::<"RemoveAll", System::Delegate, System::Delegate, System::Delegate>(a1, a2) }
    pub fn op_equality(a1: System::Delegate, a2: System::Delegate) -> bool { Self::static2::<"op_Equality", System::Delegate, System::Delegate, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Delegate, a2: System::Delegate) -> bool { Self::static2::<"op_Inequality", System::Delegate, System::Delegate, bool>(a1, a2) }
}
pub type Enum =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Enum">;
use super::*;
impl From<Enum> for System::ValueType {
 fn from(v:Enum)->System::ValueType{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ValueType,Enum>(v)
}} 
impl Enum {
    pub fn get_name(a1: System::Type, a2: System::Object) -> System::String { Self::static2::<"GetName", System::Type, System::Object, System::String>(a1, a2) }
    pub fn get_underlying_type(a1: System::Type) -> System::Type { Self::static1::<"GetUnderlyingType", System::Type, System::Type>(a1) }
    pub fn get_values(a1: System::Type) -> System::Array { Self::static1::<"GetValues", System::Type, System::Array>(a1) }
    pub fn get_values_as_underlying_type(a1: System::Type) -> System::Array { Self::static1::<"GetValuesAsUnderlyingType", System::Type, System::Array>(a1) }
    pub fn has_flag(self, a1: System::Enum) -> bool { self.instance1::<"HasFlag", System::Enum, bool>(a1) }
    pub fn is_defined(a1: System::Type, a2: System::Object) -> bool { Self::static2::<"IsDefined", System::Type, System::Object, bool>(a1, a2) }
    pub fn parse(a1: System::Type, a2: System::String) -> System::Object { Self::static2::<"Parse", System::Type, System::String, System::Object>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn compare_to(self, a1: System::Object) -> i32 { self.instance1::<"CompareTo", System::Object, i32>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn to_object(a1: System::Type, a2: System::Object) -> System::Object { Self::static2::<"ToObject", System::Type, System::Object, System::Object>(a1, a2) }
}
pub type Environment =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Environment">;
use super::*;
impl From<Environment> for System::Object {
 fn from(v:Environment)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Environment>(v)
}} 
impl Environment {
    pub fn get_current_managed_thread_id() -> i32 { Self::static0::<"get_CurrentManagedThreadId", i32>() }
    pub fn exit(a1: i32) { Self::static1::<"Exit", i32, ()>(a1) }
    pub fn get_exit_code() -> i32 { Self::static0::<"get_ExitCode", i32>() }
    pub fn set_exit_code(a1: i32) { Self::static1::<"set_ExitCode", i32, ()>(a1) }
    pub fn fail_fast(a1: System::String) { Self::static1::<"FailFast", System::String, ()>(a1) }
    pub fn get_tick_count() -> i32 { Self::static0::<"get_TickCount", i32>() }
    pub fn get_tick_count64() -> i64 { Self::static0::<"get_TickCount64", i64>() }
    pub fn get_processor_count() -> i32 { Self::static0::<"get_ProcessorCount", i32>() }
    pub fn get_is_privileged_process() -> bool { Self::static0::<"get_IsPrivilegedProcess", bool>() }
    pub fn get_has_shutdown_started() -> bool { Self::static0::<"get_HasShutdownStarted", bool>() }
    pub fn get_environment_variable(a1: System::String) -> System::String { Self::static1::<"GetEnvironmentVariable", System::String, System::String>(a1) }
    pub fn set_environment_variable(a1: System::String, a2: System::String) { Self::static2::<"SetEnvironmentVariable", System::String, System::String, ()>(a1, a2) }
    pub fn get_command_line() -> System::String { Self::static0::<"get_CommandLine", System::String>() }
    pub fn get_current_directory() -> System::String { Self::static0::<"get_CurrentDirectory", System::String>() }
    pub fn set_current_directory(a1: System::String) { Self::static1::<"set_CurrentDirectory", System::String, ()>(a1) }
    pub fn expand_environment_variables(a1: System::String) -> System::String { Self::static1::<"ExpandEnvironmentVariables", System::String, System::String>(a1) }
    pub fn get_process_id() -> i32 { Self::static0::<"get_ProcessId", i32>() }
    pub fn get_process_path() -> System::String { Self::static0::<"get_ProcessPath", System::String>() }
    pub fn get_is64_bit_process() -> bool { Self::static0::<"get_Is64BitProcess", bool>() }
    pub fn get_is64_bit_operating_system() -> bool { Self::static0::<"get_Is64BitOperatingSystem", bool>() }
    pub fn get_new_line() -> System::String { Self::static0::<"get_NewLine", System::String>() }
    pub fn get_osversion() -> System::OperatingSystem { Self::static0::<"get_OSVersion", System::OperatingSystem>() }
    pub fn get_version() -> System::Version { Self::static0::<"get_Version", System::Version>() }
    pub fn get_stack_trace() -> System::String { Self::static0::<"get_StackTrace", System::String>() }
    pub fn get_system_page_size() -> i32 { Self::static0::<"get_SystemPageSize", i32>() }
    pub fn get_environment_variables() -> System::Collections::IDictionary { Self::static0::<"GetEnvironmentVariables", System::Collections::IDictionary>() }
    pub fn get_user_interactive() -> bool { Self::static0::<"get_UserInteractive", bool>() }
    pub fn get_system_directory() -> System::String { Self::static0::<"get_SystemDirectory", System::String>() }
    pub fn get_user_domain_name() -> System::String { Self::static0::<"get_UserDomainName", System::String>() }
    pub fn get_machine_name() -> System::String { Self::static0::<"get_MachineName", System::String>() }
    pub fn get_user_name() -> System::String { Self::static0::<"get_UserName", System::String>() }
    pub fn get_working_set() -> i64 { Self::static0::<"get_WorkingSet", i64>() }
}
pub type Exception =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Exception">;
use super::*;
impl From<Exception> for System::Object {
 fn from(v:Exception)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Exception>(v)
}} 
impl Exception {
    pub fn get_target_site(self) -> System::Reflection::MethodBase { self.instance0::<"get_TargetSite", System::Reflection::MethodBase>() }
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_data(self) -> System::Collections::IDictionary { self.virt0::<"get_Data", System::Collections::IDictionary>() }
    pub fn get_base_exception(self) -> System::Exception { self.virt0::<"GetBaseException", System::Exception>() }
    pub fn get_inner_exception(self) -> System::Exception { self.instance0::<"get_InnerException", System::Exception>() }
    pub fn get_help_link(self) -> System::String { self.virt0::<"get_HelpLink", System::String>() }
    pub fn set_help_link(self, a1: System::String) { self.instance1::<"set_HelpLink", System::String, ()>(a1) }
    pub fn get_source(self) -> System::String { self.virt0::<"get_Source", System::String>() }
    pub fn set_source(self, a1: System::String) { self.instance1::<"set_Source", System::String, ()>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_hresult(self) -> i32 { self.instance0::<"get_HResult", i32>() }
    pub fn set_hresult(self, a1: i32) { self.instance1::<"set_HResult", i32, ()>(a1) }
    pub fn get_type(self) -> System::Type { self.instance0::<"GetType", System::Type>() }
    pub fn get_stack_trace(self) -> System::String { self.virt0::<"get_StackTrace", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type GC =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.GC">;
use super::*;
impl From<GC> for System::Object {
 fn from(v:GC)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,GC>(v)
}} 
impl GC {
    pub fn add_memory_pressure(a1: i64) { Self::static1::<"AddMemoryPressure", i64, ()>(a1) }
    pub fn remove_memory_pressure(a1: i64) { Self::static1::<"RemoveMemoryPressure", i64, ()>(a1) }
    pub fn get_generation(a1: System::Object) -> i32 { Self::static1::<"GetGeneration", System::Object, i32>(a1) }
    pub fn collect(a1: i32) { Self::static1::<"Collect", i32, ()>(a1) }
    pub fn collection_count(a1: i32) -> i32 { Self::static1::<"CollectionCount", i32, i32>(a1) }
    pub fn keep_alive(a1: System::Object) { Self::static1::<"KeepAlive", System::Object, ()>(a1) }
    pub fn get_max_generation() -> i32 { Self::static0::<"get_MaxGeneration", i32>() }
    pub fn wait_for_pending_finalizers() { Self::static0::<"WaitForPendingFinalizers", ()>() }
    pub fn suppress_finalize(a1: System::Object) { Self::static1::<"SuppressFinalize", System::Object, ()>(a1) }
    pub fn re_register_for_finalize(a1: System::Object) { Self::static1::<"ReRegisterForFinalize", System::Object, ()>(a1) }
    pub fn get_total_memory(a1: bool) -> i64 { Self::static1::<"GetTotalMemory", bool, i64>(a1) }
    pub fn get_allocated_bytes_for_current_thread() -> i64 { Self::static0::<"GetAllocatedBytesForCurrentThread", i64>() }
    pub fn get_total_allocated_bytes(a1: bool) -> i64 { Self::static1::<"GetTotalAllocatedBytes", bool, i64>(a1) }
    pub fn register_for_full_gcnotification(a1: i32, a2: i32) { Self::static2::<"RegisterForFullGCNotification", i32, i32, ()>(a1, a2) }
    pub fn cancel_full_gcnotification() { Self::static0::<"CancelFullGCNotification", ()>() }
    pub fn try_start_no_gcregion(a1: i64) -> bool { Self::static1::<"TryStartNoGCRegion", i64, bool>(a1) }
    pub fn end_no_gcregion() { Self::static0::<"EndNoGCRegion", ()>() }
    pub fn register_no_gcregion_callback(a1: i64, a2: System::Action) { Self::static2::<"RegisterNoGCRegionCallback", i64, System::Action, ()>(a1, a2) }
    pub fn refresh_memory_limit() { Self::static0::<"RefreshMemoryLimit", ()>() }
}
pub type Math =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Math">;
use super::*;
impl From<Math> for System::Object {
 fn from(v:Math)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Math>(v)
}} 
impl Math {
    pub fn acos(a1: f64) -> f64 { Self::static1::<"Acos", f64, f64>(a1) }
    pub fn acosh(a1: f64) -> f64 { Self::static1::<"Acosh", f64, f64>(a1) }
    pub fn asin(a1: f64) -> f64 { Self::static1::<"Asin", f64, f64>(a1) }
    pub fn asinh(a1: f64) -> f64 { Self::static1::<"Asinh", f64, f64>(a1) }
    pub fn atan(a1: f64) -> f64 { Self::static1::<"Atan", f64, f64>(a1) }
    pub fn atanh(a1: f64) -> f64 { Self::static1::<"Atanh", f64, f64>(a1) }
    pub fn atan2(a1: f64, a2: f64) -> f64 { Self::static2::<"Atan2", f64, f64, f64>(a1, a2) }
    pub fn cbrt(a1: f64) -> f64 { Self::static1::<"Cbrt", f64, f64>(a1) }
    pub fn ceiling(a1: f64) -> f64 { Self::static1::<"Ceiling", f64, f64>(a1) }
    pub fn cos(a1: f64) -> f64 { Self::static1::<"Cos", f64, f64>(a1) }
    pub fn cosh(a1: f64) -> f64 { Self::static1::<"Cosh", f64, f64>(a1) }
    pub fn exp(a1: f64) -> f64 { Self::static1::<"Exp", f64, f64>(a1) }
    pub fn floor(a1: f64) -> f64 { Self::static1::<"Floor", f64, f64>(a1) }
    pub fn log(a1: f64) -> f64 { Self::static1::<"Log", f64, f64>(a1) }
    pub fn log2(a1: f64) -> f64 { Self::static1::<"Log2", f64, f64>(a1) }
    pub fn log10(a1: f64) -> f64 { Self::static1::<"Log10", f64, f64>(a1) }
    pub fn pow(a1: f64, a2: f64) -> f64 { Self::static2::<"Pow", f64, f64, f64>(a1, a2) }
    pub fn sin(a1: f64) -> f64 { Self::static1::<"Sin", f64, f64>(a1) }
    pub fn sinh(a1: f64) -> f64 { Self::static1::<"Sinh", f64, f64>(a1) }
    pub fn sqrt(a1: f64) -> f64 { Self::static1::<"Sqrt", f64, f64>(a1) }
    pub fn tan(a1: f64) -> f64 { Self::static1::<"Tan", f64, f64>(a1) }
    pub fn tanh(a1: f64) -> f64 { Self::static1::<"Tanh", f64, f64>(a1) }
    pub fn abs(a1: i16) -> i16 { Self::static1::<"Abs", i16, i16>(a1) }
    pub fn big_mul(a1: i32, a2: i32) -> i64 { Self::static2::<"BigMul", i32, i32, i64>(a1, a2) }
    pub fn bit_decrement(a1: f64) -> f64 { Self::static1::<"BitDecrement", f64, f64>(a1) }
    pub fn bit_increment(a1: f64) -> f64 { Self::static1::<"BitIncrement", f64, f64>(a1) }
    pub fn copy_sign(a1: f64, a2: f64) -> f64 { Self::static2::<"CopySign", f64, f64, f64>(a1, a2) }
    pub fn ieeeremainder(a1: f64, a2: f64) -> f64 { Self::static2::<"IEEERemainder", f64, f64, f64>(a1, a2) }
    pub fn ilog_b(a1: f64) -> i32 { Self::static1::<"ILogB", f64, i32>(a1) }
    pub fn max(a1: u8, a2: u8) -> u8 { Self::static2::<"Max", u8, u8, u8>(a1, a2) }
    pub fn max_magnitude(a1: f64, a2: f64) -> f64 { Self::static2::<"MaxMagnitude", f64, f64, f64>(a1, a2) }
    pub fn min(a1: u8, a2: u8) -> u8 { Self::static2::<"Min", u8, u8, u8>(a1, a2) }
    pub fn min_magnitude(a1: f64, a2: f64) -> f64 { Self::static2::<"MinMagnitude", f64, f64, f64>(a1, a2) }
    pub fn reciprocal_estimate(a1: f64) -> f64 { Self::static1::<"ReciprocalEstimate", f64, f64>(a1) }
    pub fn reciprocal_sqrt_estimate(a1: f64) -> f64 { Self::static1::<"ReciprocalSqrtEstimate", f64, f64>(a1) }
    pub fn round(a1: f64) -> f64 { Self::static1::<"Round", f64, f64>(a1) }
    pub fn sign(a1: f64) -> i32 { Self::static1::<"Sign", f64, i32>(a1) }
    pub fn truncate(a1: f64) -> f64 { Self::static1::<"Truncate", f64, f64>(a1) }
    pub fn scale_b(a1: f64, a2: i32) -> f64 { Self::static2::<"ScaleB", f64, i32, f64>(a1, a2) }
}
pub type MathF =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MathF">;
use super::*;
impl From<MathF> for System::Object {
 fn from(v:MathF)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MathF>(v)
}} 
impl MathF {
    pub fn acos(a1: f32) -> f32 { Self::static1::<"Acos", f32, f32>(a1) }
    pub fn acosh(a1: f32) -> f32 { Self::static1::<"Acosh", f32, f32>(a1) }
    pub fn asin(a1: f32) -> f32 { Self::static1::<"Asin", f32, f32>(a1) }
    pub fn asinh(a1: f32) -> f32 { Self::static1::<"Asinh", f32, f32>(a1) }
    pub fn atan(a1: f32) -> f32 { Self::static1::<"Atan", f32, f32>(a1) }
    pub fn atanh(a1: f32) -> f32 { Self::static1::<"Atanh", f32, f32>(a1) }
    pub fn atan2(a1: f32, a2: f32) -> f32 { Self::static2::<"Atan2", f32, f32, f32>(a1, a2) }
    pub fn cbrt(a1: f32) -> f32 { Self::static1::<"Cbrt", f32, f32>(a1) }
    pub fn ceiling(a1: f32) -> f32 { Self::static1::<"Ceiling", f32, f32>(a1) }
    pub fn cos(a1: f32) -> f32 { Self::static1::<"Cos", f32, f32>(a1) }
    pub fn cosh(a1: f32) -> f32 { Self::static1::<"Cosh", f32, f32>(a1) }
    pub fn exp(a1: f32) -> f32 { Self::static1::<"Exp", f32, f32>(a1) }
    pub fn floor(a1: f32) -> f32 { Self::static1::<"Floor", f32, f32>(a1) }
    pub fn log(a1: f32) -> f32 { Self::static1::<"Log", f32, f32>(a1) }
    pub fn log2(a1: f32) -> f32 { Self::static1::<"Log2", f32, f32>(a1) }
    pub fn log10(a1: f32) -> f32 { Self::static1::<"Log10", f32, f32>(a1) }
    pub fn pow(a1: f32, a2: f32) -> f32 { Self::static2::<"Pow", f32, f32, f32>(a1, a2) }
    pub fn sin(a1: f32) -> f32 { Self::static1::<"Sin", f32, f32>(a1) }
    pub fn sinh(a1: f32) -> f32 { Self::static1::<"Sinh", f32, f32>(a1) }
    pub fn sqrt(a1: f32) -> f32 { Self::static1::<"Sqrt", f32, f32>(a1) }
    pub fn tan(a1: f32) -> f32 { Self::static1::<"Tan", f32, f32>(a1) }
    pub fn tanh(a1: f32) -> f32 { Self::static1::<"Tanh", f32, f32>(a1) }
    pub fn abs(a1: f32) -> f32 { Self::static1::<"Abs", f32, f32>(a1) }
    pub fn bit_decrement(a1: f32) -> f32 { Self::static1::<"BitDecrement", f32, f32>(a1) }
    pub fn bit_increment(a1: f32) -> f32 { Self::static1::<"BitIncrement", f32, f32>(a1) }
    pub fn copy_sign(a1: f32, a2: f32) -> f32 { Self::static2::<"CopySign", f32, f32, f32>(a1, a2) }
    pub fn ieeeremainder(a1: f32, a2: f32) -> f32 { Self::static2::<"IEEERemainder", f32, f32, f32>(a1, a2) }
    pub fn ilog_b(a1: f32) -> i32 { Self::static1::<"ILogB", f32, i32>(a1) }
    pub fn max(a1: f32, a2: f32) -> f32 { Self::static2::<"Max", f32, f32, f32>(a1, a2) }
    pub fn max_magnitude(a1: f32, a2: f32) -> f32 { Self::static2::<"MaxMagnitude", f32, f32, f32>(a1, a2) }
    pub fn min(a1: f32, a2: f32) -> f32 { Self::static2::<"Min", f32, f32, f32>(a1, a2) }
    pub fn min_magnitude(a1: f32, a2: f32) -> f32 { Self::static2::<"MinMagnitude", f32, f32, f32>(a1, a2) }
    pub fn reciprocal_estimate(a1: f32) -> f32 { Self::static1::<"ReciprocalEstimate", f32, f32>(a1) }
    pub fn reciprocal_sqrt_estimate(a1: f32) -> f32 { Self::static1::<"ReciprocalSqrtEstimate", f32, f32>(a1) }
    pub fn round(a1: f32) -> f32 { Self::static1::<"Round", f32, f32>(a1) }
    pub fn sign(a1: f32) -> i32 { Self::static1::<"Sign", f32, i32>(a1) }
    pub fn truncate(a1: f32) -> f32 { Self::static1::<"Truncate", f32, f32>(a1) }
    pub fn scale_b(a1: f32, a2: i32) -> f32 { Self::static2::<"ScaleB", f32, i32, f32>(a1, a2) }
}
pub type MulticastDelegate =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MulticastDelegate">;
use super::*;
impl From<MulticastDelegate> for System::Delegate {
 fn from(v:MulticastDelegate)->System::Delegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Delegate,MulticastDelegate>(v)
}} 
impl MulticastDelegate {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn op_equality(a1: System::MulticastDelegate, a2: System::MulticastDelegate) -> bool { Self::static2::<"op_Equality", System::MulticastDelegate, System::MulticastDelegate, bool>(a1, a2) }
    pub fn op_inequality(a1: System::MulticastDelegate, a2: System::MulticastDelegate) -> bool { Self::static2::<"op_Inequality", System::MulticastDelegate, System::MulticastDelegate, bool>(a1, a2) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
}
pub type Object =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Object">;
use super::*;
impl Object {
    pub fn get_type(self) -> System::Type { self.instance0::<"GetType", System::Type>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn reference_equals(a1: System::Object, a2: System::Object) -> bool { Self::static2::<"ReferenceEquals", System::Object, System::Object, bool>(a1, a2) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type String =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.String">;
use super::*;
impl From<String> for System::Object {
 fn from(v:String)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,String>(v)
}} 
impl String {
    pub fn intern(a1: System::String) -> System::String { Self::static1::<"Intern", System::String, System::String>(a1) }
    pub fn is_interned(a1: System::String) -> System::String { Self::static1::<"IsInterned", System::String, System::String>(a1) }
    pub fn compare(a1: System::String, a2: System::String) -> i32 { Self::static2::<"Compare", System::String, System::String, i32>(a1, a2) }
    pub fn compare_ordinal(a1: System::String, a2: System::String) -> i32 { Self::static2::<"CompareOrdinal", System::String, System::String, i32>(a1, a2) }
    pub fn compare_to(self, a1: System::Object) -> i32 { self.instance1::<"CompareTo", System::Object, i32>(a1) }
    pub fn ends_with(self, a1: System::String) -> bool { self.instance1::<"EndsWith", System::String, bool>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn op_equality(a1: System::String, a2: System::String) -> bool { Self::static2::<"op_Equality", System::String, System::String, bool>(a1, a2) }
    pub fn op_inequality(a1: System::String, a2: System::String) -> bool { Self::static2::<"op_Inequality", System::String, System::String, bool>(a1, a2) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn starts_with(self, a1: System::String) -> bool { self.instance1::<"StartsWith", System::String, bool>(a1) }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn copy(a1: System::String) -> System::String { Self::static1::<"Copy", System::String, System::String>(a1) }
    pub fn is_null_or_empty(a1: System::String) -> bool { Self::static1::<"IsNullOrEmpty", System::String, bool>(a1) }
    pub fn is_null_or_white_space(a1: System::String) -> bool { Self::static1::<"IsNullOrWhiteSpace", System::String, bool>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_enumerator(self) -> System::CharEnumerator { self.instance0::<"GetEnumerator", System::CharEnumerator>() }
    pub fn is_normalized(self) -> bool { self.instance0::<"IsNormalized", bool>() }
    pub fn normalize(self) -> System::String { self.instance0::<"Normalize", System::String>() }
    pub fn get_length(self) -> i32 { self.instance0::<"get_Length", i32>() }
    pub fn concat(a1: System::Object) -> System::String { Self::static1::<"Concat", System::Object, System::String>(a1) }
    pub fn format(a1: System::String, a2: System::Object) -> System::String { Self::static2::<"Format", System::String, System::Object, System::String>(a1, a2) }
    pub fn insert(self, a1: i32, a2: System::String) -> System::String { self.instance2::<"Insert", i32, System::String, System::String>(a1, a2) }
    pub fn pad_left(self, a1: i32) -> System::String { self.instance1::<"PadLeft", i32, System::String>(a1) }
    pub fn pad_right(self, a1: i32) -> System::String { self.instance1::<"PadRight", i32, System::String>(a1) }
    pub fn remove(self, a1: i32, a2: i32) -> System::String { self.instance2::<"Remove", i32, i32, System::String>(a1, a2) }
    pub fn replace(self, a1: System::String, a2: System::String) -> System::String { self.instance2::<"Replace", System::String, System::String, System::String>(a1, a2) }
    pub fn replace_line_endings(self) -> System::String { self.instance0::<"ReplaceLineEndings", System::String>() }
    pub fn substring(self, a1: i32) -> System::String { self.instance1::<"Substring", i32, System::String>(a1) }
    pub fn to_lower(self) -> System::String { self.instance0::<"ToLower", System::String>() }
    pub fn to_lower_invariant(self) -> System::String { self.instance0::<"ToLowerInvariant", System::String>() }
    pub fn to_upper(self) -> System::String { self.instance0::<"ToUpper", System::String>() }
    pub fn to_upper_invariant(self) -> System::String { self.instance0::<"ToUpperInvariant", System::String>() }
    pub fn trim(self) -> System::String { self.instance0::<"Trim", System::String>() }
    pub fn trim_start(self) -> System::String { self.instance0::<"TrimStart", System::String>() }
    pub fn trim_end(self) -> System::String { self.instance0::<"TrimEnd", System::String>() }
    pub fn contains(self, a1: System::String) -> bool { self.instance1::<"Contains", System::String, bool>(a1) }
    pub fn index_of(self, a1: System::String) -> i32 { self.instance1::<"IndexOf", System::String, i32>(a1) }
    pub fn last_index_of(self, a1: System::String) -> i32 { self.instance1::<"LastIndexOf", System::String, i32>(a1) }
}
pub type Type =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Type">;
use super::*;
impl From<Type> for System::Reflection::MemberInfo {
 fn from(v:Type)->System::Reflection::MemberInfo{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Reflection::MemberInfo,Type>(v)
}} 
impl Type {
    pub fn get_is_interface(self) -> bool { self.instance0::<"get_IsInterface", bool>() }
    pub fn get_type(a1: System::String, a2: bool) -> System::Type { Self::static2::<"GetType", System::String, bool, System::Type>(a1, a2) }
    pub fn get_namespace(self) -> System::String { self.virt0::<"get_Namespace", System::String>() }
    pub fn get_assembly_qualified_name(self) -> System::String { self.virt0::<"get_AssemblyQualifiedName", System::String>() }
    pub fn get_full_name(self) -> System::String { self.virt0::<"get_FullName", System::String>() }
    pub fn get_assembly(self) -> System::Reflection::Assembly { self.virt0::<"get_Assembly", System::Reflection::Assembly>() }
    pub fn get_module(self) -> System::Reflection::Module { self.virt0::<"get_Module", System::Reflection::Module>() }
    pub fn get_is_nested(self) -> bool { self.instance0::<"get_IsNested", bool>() }
    pub fn get_declaring_type(self) -> System::Type { self.virt0::<"get_DeclaringType", System::Type>() }
    pub fn get_declaring_method(self) -> System::Reflection::MethodBase { self.virt0::<"get_DeclaringMethod", System::Reflection::MethodBase>() }
    pub fn get_reflected_type(self) -> System::Type { self.virt0::<"get_ReflectedType", System::Type>() }
    pub fn get_underlying_system_type(self) -> System::Type { self.virt0::<"get_UnderlyingSystemType", System::Type>() }
    pub fn get_is_type_definition(self) -> bool { self.virt0::<"get_IsTypeDefinition", bool>() }
    pub fn get_is_array(self) -> bool { self.instance0::<"get_IsArray", bool>() }
    pub fn get_is_by_ref(self) -> bool { self.instance0::<"get_IsByRef", bool>() }
    pub fn get_is_pointer(self) -> bool { self.instance0::<"get_IsPointer", bool>() }
    pub fn get_is_constructed_generic_type(self) -> bool { self.virt0::<"get_IsConstructedGenericType", bool>() }
    pub fn get_is_generic_parameter(self) -> bool { self.virt0::<"get_IsGenericParameter", bool>() }
    pub fn get_is_generic_type_parameter(self) -> bool { self.virt0::<"get_IsGenericTypeParameter", bool>() }
    pub fn get_is_generic_method_parameter(self) -> bool { self.virt0::<"get_IsGenericMethodParameter", bool>() }
    pub fn get_is_generic_type(self) -> bool { self.virt0::<"get_IsGenericType", bool>() }
    pub fn get_is_generic_type_definition(self) -> bool { self.virt0::<"get_IsGenericTypeDefinition", bool>() }
    pub fn get_is_szarray(self) -> bool { self.virt0::<"get_IsSZArray", bool>() }
    pub fn get_is_variable_bound_array(self) -> bool { self.virt0::<"get_IsVariableBoundArray", bool>() }
    pub fn get_is_by_ref_like(self) -> bool { self.virt0::<"get_IsByRefLike", bool>() }
    pub fn get_is_function_pointer(self) -> bool { self.virt0::<"get_IsFunctionPointer", bool>() }
    pub fn get_is_unmanaged_function_pointer(self) -> bool { self.virt0::<"get_IsUnmanagedFunctionPointer", bool>() }
    pub fn get_has_element_type(self) -> bool { self.instance0::<"get_HasElementType", bool>() }
    pub fn get_element_type(self) -> System::Type { self.virt0::<"GetElementType", System::Type>() }
    pub fn get_array_rank(self) -> i32 { self.virt0::<"GetArrayRank", i32>() }
    pub fn get_generic_type_definition(self) -> System::Type { self.virt0::<"GetGenericTypeDefinition", System::Type>() }
    pub fn get_generic_parameter_position(self) -> i32 { self.virt0::<"get_GenericParameterPosition", i32>() }
    pub fn get_is_abstract(self) -> bool { self.instance0::<"get_IsAbstract", bool>() }
    pub fn get_is_import(self) -> bool { self.instance0::<"get_IsImport", bool>() }
    pub fn get_is_sealed(self) -> bool { self.instance0::<"get_IsSealed", bool>() }
    pub fn get_is_special_name(self) -> bool { self.instance0::<"get_IsSpecialName", bool>() }
    pub fn get_is_class(self) -> bool { self.instance0::<"get_IsClass", bool>() }
    pub fn get_is_nested_assembly(self) -> bool { self.instance0::<"get_IsNestedAssembly", bool>() }
    pub fn get_is_nested_fam_andassem(self) -> bool { self.instance0::<"get_IsNestedFamANDAssem", bool>() }
    pub fn get_is_nested_family(self) -> bool { self.instance0::<"get_IsNestedFamily", bool>() }
    pub fn get_is_nested_fam_orassem(self) -> bool { self.instance0::<"get_IsNestedFamORAssem", bool>() }
    pub fn get_is_nested_private(self) -> bool { self.instance0::<"get_IsNestedPrivate", bool>() }
    pub fn get_is_nested_public(self) -> bool { self.instance0::<"get_IsNestedPublic", bool>() }
    pub fn get_is_not_public(self) -> bool { self.instance0::<"get_IsNotPublic", bool>() }
    pub fn get_is_public(self) -> bool { self.instance0::<"get_IsPublic", bool>() }
    pub fn get_is_auto_layout(self) -> bool { self.instance0::<"get_IsAutoLayout", bool>() }
    pub fn get_is_explicit_layout(self) -> bool { self.instance0::<"get_IsExplicitLayout", bool>() }
    pub fn get_is_layout_sequential(self) -> bool { self.instance0::<"get_IsLayoutSequential", bool>() }
    pub fn get_is_ansi_class(self) -> bool { self.instance0::<"get_IsAnsiClass", bool>() }
    pub fn get_is_auto_class(self) -> bool { self.instance0::<"get_IsAutoClass", bool>() }
    pub fn get_is_unicode_class(self) -> bool { self.instance0::<"get_IsUnicodeClass", bool>() }
    pub fn get_is_comobject(self) -> bool { self.instance0::<"get_IsCOMObject", bool>() }
    pub fn get_is_contextful(self) -> bool { self.instance0::<"get_IsContextful", bool>() }
    pub fn get_is_enum(self) -> bool { self.virt0::<"get_IsEnum", bool>() }
    pub fn get_is_marshal_by_ref(self) -> bool { self.instance0::<"get_IsMarshalByRef", bool>() }
    pub fn get_is_primitive(self) -> bool { self.instance0::<"get_IsPrimitive", bool>() }
    pub fn get_is_value_type(self) -> bool { self.instance0::<"get_IsValueType", bool>() }
    pub fn is_assignable_to(self, a1: System::Type) -> bool { self.instance1::<"IsAssignableTo", System::Type, bool>(a1) }
    pub fn get_is_signature_type(self) -> bool { self.virt0::<"get_IsSignatureType", bool>() }
    pub fn get_is_security_critical(self) -> bool { self.virt0::<"get_IsSecurityCritical", bool>() }
    pub fn get_is_security_safe_critical(self) -> bool { self.virt0::<"get_IsSecuritySafeCritical", bool>() }
    pub fn get_is_security_transparent(self) -> bool { self.virt0::<"get_IsSecurityTransparent", bool>() }
    pub fn get_struct_layout_attribute(self) -> System::Runtime::InteropServices::StructLayoutAttribute { self.virt0::<"get_StructLayoutAttribute", System::Runtime::InteropServices::StructLayoutAttribute>() }
    pub fn get_type_initializer(self) -> System::Reflection::ConstructorInfo { self.instance0::<"get_TypeInitializer", System::Reflection::ConstructorInfo>() }
    pub fn get_event(self, a1: System::String) -> System::Reflection::EventInfo { self.instance1::<"GetEvent", System::String, System::Reflection::EventInfo>(a1) }
    pub fn get_field(self, a1: System::String) -> System::Reflection::FieldInfo { self.instance1::<"GetField", System::String, System::Reflection::FieldInfo>(a1) }
    pub fn get_function_pointer_return_type(self) -> System::Type { self.virt0::<"GetFunctionPointerReturnType", System::Type>() }
    pub fn get_member_with_same_metadata_definition_as(self, a1: System::Reflection::MemberInfo) -> System::Reflection::MemberInfo { self.instance1::<"GetMemberWithSameMetadataDefinitionAs", System::Reflection::MemberInfo, System::Reflection::MemberInfo>(a1) }
    pub fn get_method(self, a1: System::String) -> System::Reflection::MethodInfo { self.instance1::<"GetMethod", System::String, System::Reflection::MethodInfo>(a1) }
    pub fn get_nested_type(self, a1: System::String) -> System::Type { self.instance1::<"GetNestedType", System::String, System::Type>(a1) }
    pub fn get_property(self, a1: System::String) -> System::Reflection::PropertyInfo { self.instance1::<"GetProperty", System::String, System::Reflection::PropertyInfo>(a1) }
    pub fn get_type_from_prog_id(a1: System::String) -> System::Type { Self::static1::<"GetTypeFromProgID", System::String, System::Type>(a1) }
    pub fn get_base_type(self) -> System::Type { self.virt0::<"get_BaseType", System::Type>() }
    pub fn get_interface(self, a1: System::String) -> System::Type { self.instance1::<"GetInterface", System::String, System::Type>(a1) }
    pub fn is_instance_of_type(self, a1: System::Object) -> bool { self.instance1::<"IsInstanceOfType", System::Object, bool>(a1) }
    pub fn is_equivalent_to(self, a1: System::Type) -> bool { self.instance1::<"IsEquivalentTo", System::Type, bool>(a1) }
    pub fn get_enum_underlying_type(self) -> System::Type { self.virt0::<"GetEnumUnderlyingType", System::Type>() }
    pub fn get_enum_values(self) -> System::Array { self.virt0::<"GetEnumValues", System::Array>() }
    pub fn get_enum_values_as_underlying_type(self) -> System::Array { self.virt0::<"GetEnumValuesAsUnderlyingType", System::Array>() }
    pub fn make_array_type(self) -> System::Type { self.virt0::<"MakeArrayType", System::Type>() }
    pub fn make_by_ref_type(self) -> System::Type { self.virt0::<"MakeByRefType", System::Type>() }
    pub fn make_pointer_type(self) -> System::Type { self.virt0::<"MakePointerType", System::Type>() }
    pub fn make_generic_method_parameter(a1: i32) -> System::Type { Self::static1::<"MakeGenericMethodParameter", i32, System::Type>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn op_equality(a1: System::Type, a2: System::Type) -> bool { Self::static2::<"op_Equality", System::Type, System::Type, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Type, a2: System::Type) -> bool { Self::static2::<"op_Inequality", System::Type, System::Type, bool>(a1, a2) }
    pub fn get_default_binder() -> System::Reflection::Binder { Self::static0::<"get_DefaultBinder", System::Reflection::Binder>() }
    pub fn is_enum_defined(self, a1: System::Object) -> bool { self.instance1::<"IsEnumDefined", System::Object, bool>(a1) }
    pub fn get_enum_name(self, a1: System::Object) -> System::String { self.instance1::<"GetEnumName", System::Object, System::String>(a1) }
    pub fn get_is_serializable(self) -> bool { self.virt0::<"get_IsSerializable", bool>() }
    pub fn get_contains_generic_parameters(self) -> bool { self.virt0::<"get_ContainsGenericParameters", bool>() }
    pub fn get_is_visible(self) -> bool { self.instance0::<"get_IsVisible", bool>() }
    pub fn is_subclass_of(self, a1: System::Type) -> bool { self.instance1::<"IsSubclassOf", System::Type, bool>(a1) }
    pub fn is_assignable_from(self, a1: System::Type) -> bool { self.instance1::<"IsAssignableFrom", System::Type, bool>(a1) }
}
pub type TypeLoadException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TypeLoadException">;
use super::*;
impl From<TypeLoadException> for System::SystemException {
 fn from(v:TypeLoadException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,TypeLoadException>(v)
}} 
impl TypeLoadException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_type_name(self) -> System::String { self.instance0::<"get_TypeName", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ValueType =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ValueType">;
use super::*;
impl From<ValueType> for System::Object {
 fn from(v:ValueType)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ValueType>(v)
}} 
impl ValueType {
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type AccessViolationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AccessViolationException">;
use super::*;
impl From<AccessViolationException> for System::SystemException {
 fn from(v:AccessViolationException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,AccessViolationException>(v)
}} 
impl AccessViolationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Action =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Action">;
use super::*;
impl From<Action> for System::MulticastDelegate {
 fn from(v:Action)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,Action>(v)
}} 
impl Action {
    pub fn invoke(self) { self.virt0::<"Invoke", ()>() }
    pub fn begin_invoke(self, a1: System::AsyncCallback, a2: System::Object) -> System::IAsyncResult { self.instance2::<"BeginInvoke", System::AsyncCallback, System::Object, System::IAsyncResult>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type Activator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Activator">;
use super::*;
impl From<Activator> for System::Object {
 fn from(v:Activator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Activator>(v)
}} 
impl Activator {
    pub fn create_instance(a1: System::Type) -> System::Object { Self::static1::<"CreateInstance", System::Type, System::Object>(a1) }
    pub fn create_instance_from(a1: System::String, a2: System::String) -> System::Runtime::Remoting::ObjectHandle { Self::static2::<"CreateInstanceFrom", System::String, System::String, System::Runtime::Remoting::ObjectHandle>(a1, a2) }
}
pub type AggregateException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AggregateException">;
use super::*;
impl From<AggregateException> for System::Exception {
 fn from(v:AggregateException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,AggregateException>(v)
}} 
impl AggregateException {
    pub fn get_base_exception(self) -> System::Exception { self.virt0::<"GetBaseException", System::Exception>() }
    pub fn flatten(self) -> System::AggregateException { self.instance0::<"Flatten", System::AggregateException>() }
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type AppContext =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AppContext">;
use super::*;
impl From<AppContext> for System::Object {
 fn from(v:AppContext)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AppContext>(v)
}} 
impl AppContext {
    pub fn get_base_directory() -> System::String { Self::static0::<"get_BaseDirectory", System::String>() }
    pub fn get_target_framework_name() -> System::String { Self::static0::<"get_TargetFrameworkName", System::String>() }
    pub fn get_data(a1: System::String) -> System::Object { Self::static1::<"GetData", System::String, System::Object>(a1) }
    pub fn set_data(a1: System::String, a2: System::Object) { Self::static2::<"SetData", System::String, System::Object, ()>(a1, a2) }
    pub fn set_switch(a1: System::String, a2: bool) { Self::static2::<"SetSwitch", System::String, bool, ()>(a1, a2) }
}
pub type AppDomain =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AppDomain">;
use super::*;
impl From<AppDomain> for System::MarshalByRefObject {
 fn from(v:AppDomain)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,AppDomain>(v)
}} 
impl AppDomain {
    pub fn get_current_domain() -> System::AppDomain { Self::static0::<"get_CurrentDomain", System::AppDomain>() }
    pub fn get_base_directory(self) -> System::String { self.instance0::<"get_BaseDirectory", System::String>() }
    pub fn get_relative_search_path(self) -> System::String { self.instance0::<"get_RelativeSearchPath", System::String>() }
    pub fn get_setup_information(self) -> System::AppDomainSetup { self.instance0::<"get_SetupInformation", System::AppDomainSetup>() }
    pub fn get_permission_set(self) -> System::Security::PermissionSet { self.instance0::<"get_PermissionSet", System::Security::PermissionSet>() }
    pub fn add_unhandled_exception(self, a1: System::UnhandledExceptionEventHandler) { self.instance1::<"add_UnhandledException", System::UnhandledExceptionEventHandler, ()>(a1) }
    pub fn remove_unhandled_exception(self, a1: System::UnhandledExceptionEventHandler) { self.instance1::<"remove_UnhandledException", System::UnhandledExceptionEventHandler, ()>(a1) }
    pub fn get_dynamic_directory(self) -> System::String { self.instance0::<"get_DynamicDirectory", System::String>() }
    pub fn set_dynamic_base(self, a1: System::String) { self.instance1::<"SetDynamicBase", System::String, ()>(a1) }
    pub fn get_friendly_name(self) -> System::String { self.instance0::<"get_FriendlyName", System::String>() }
    pub fn get_id(self) -> i32 { self.instance0::<"get_Id", i32>() }
    pub fn get_is_fully_trusted(self) -> bool { self.instance0::<"get_IsFullyTrusted", bool>() }
    pub fn get_is_homogenous(self) -> bool { self.instance0::<"get_IsHomogenous", bool>() }
    pub fn add_domain_unload(self, a1: System::EventHandler) { self.instance1::<"add_DomainUnload", System::EventHandler, ()>(a1) }
    pub fn remove_domain_unload(self, a1: System::EventHandler) { self.instance1::<"remove_DomainUnload", System::EventHandler, ()>(a1) }
    pub fn add_process_exit(self, a1: System::EventHandler) { self.instance1::<"add_ProcessExit", System::EventHandler, ()>(a1) }
    pub fn remove_process_exit(self, a1: System::EventHandler) { self.instance1::<"remove_ProcessExit", System::EventHandler, ()>(a1) }
    pub fn apply_policy(self, a1: System::String) -> System::String { self.instance1::<"ApplyPolicy", System::String, System::String>(a1) }
    pub fn create_domain(a1: System::String) -> System::AppDomain { Self::static1::<"CreateDomain", System::String, System::AppDomain>(a1) }
    pub fn execute_assembly(self, a1: System::String) -> i32 { self.instance1::<"ExecuteAssembly", System::String, i32>(a1) }
    pub fn execute_assembly_by_name(self, a1: System::String) -> i32 { self.instance1::<"ExecuteAssemblyByName", System::String, i32>(a1) }
    pub fn get_data(self, a1: System::String) -> System::Object { self.instance1::<"GetData", System::String, System::Object>(a1) }
    pub fn set_data(self, a1: System::String, a2: System::Object) { self.instance2::<"SetData", System::String, System::Object, ()>(a1, a2) }
    pub fn is_default_app_domain(self) -> bool { self.instance0::<"IsDefaultAppDomain", bool>() }
    pub fn is_finalizing_for_unload(self) -> bool { self.instance0::<"IsFinalizingForUnload", bool>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn unload(a1: System::AppDomain) { Self::static1::<"Unload", System::AppDomain, ()>(a1) }
    pub fn load(self, a1: System::Reflection::AssemblyName) -> System::Reflection::Assembly { self.instance1::<"Load", System::Reflection::AssemblyName, System::Reflection::Assembly>(a1) }
    pub fn get_monitoring_is_enabled() -> bool { Self::static0::<"get_MonitoringIsEnabled", bool>() }
    pub fn set_monitoring_is_enabled(a1: bool) { Self::static1::<"set_MonitoringIsEnabled", bool, ()>(a1) }
    pub fn get_monitoring_survived_memory_size(self) -> i64 { self.instance0::<"get_MonitoringSurvivedMemorySize", i64>() }
    pub fn get_monitoring_survived_process_memory_size() -> i64 { Self::static0::<"get_MonitoringSurvivedProcessMemorySize", i64>() }
    pub fn get_monitoring_total_allocated_memory_size(self) -> i64 { self.instance0::<"get_MonitoringTotalAllocatedMemorySize", i64>() }
    pub fn get_current_thread_id() -> i32 { Self::static0::<"GetCurrentThreadId", i32>() }
    pub fn get_shadow_copy_files(self) -> bool { self.instance0::<"get_ShadowCopyFiles", bool>() }
    pub fn append_private_path(self, a1: System::String) { self.instance1::<"AppendPrivatePath", System::String, ()>(a1) }
    pub fn clear_private_path(self) { self.instance0::<"ClearPrivatePath", ()>() }
    pub fn clear_shadow_copy_path(self) { self.instance0::<"ClearShadowCopyPath", ()>() }
    pub fn set_cache_path(self, a1: System::String) { self.instance1::<"SetCachePath", System::String, ()>(a1) }
    pub fn set_shadow_copy_files(self) { self.instance0::<"SetShadowCopyFiles", ()>() }
    pub fn set_shadow_copy_path(self, a1: System::String) { self.instance1::<"SetShadowCopyPath", System::String, ()>(a1) }
    pub fn add_assembly_load(self, a1: System::AssemblyLoadEventHandler) { self.instance1::<"add_AssemblyLoad", System::AssemblyLoadEventHandler, ()>(a1) }
    pub fn remove_assembly_load(self, a1: System::AssemblyLoadEventHandler) { self.instance1::<"remove_AssemblyLoad", System::AssemblyLoadEventHandler, ()>(a1) }
    pub fn add_assembly_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"add_AssemblyResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn remove_assembly_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"remove_AssemblyResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn add_reflection_only_assembly_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"add_ReflectionOnlyAssemblyResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn remove_reflection_only_assembly_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"remove_ReflectionOnlyAssemblyResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn add_type_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"add_TypeResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn remove_type_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"remove_TypeResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn add_resource_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"add_ResourceResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn remove_resource_resolve(self, a1: System::ResolveEventHandler) { self.instance1::<"remove_ResourceResolve", System::ResolveEventHandler, ()>(a1) }
    pub fn set_thread_principal(self, a1: System::Security::Principal::IPrincipal) { self.instance1::<"SetThreadPrincipal", System::Security::Principal::IPrincipal, ()>(a1) }
    pub fn create_instance(self, a1: System::String, a2: System::String) -> System::Runtime::Remoting::ObjectHandle { self.instance2::<"CreateInstance", System::String, System::String, System::Runtime::Remoting::ObjectHandle>(a1, a2) }
    pub fn create_instance_and_unwrap(self, a1: System::String, a2: System::String) -> System::Object { self.instance2::<"CreateInstanceAndUnwrap", System::String, System::String, System::Object>(a1, a2) }
    pub fn create_instance_from(self, a1: System::String, a2: System::String) -> System::Runtime::Remoting::ObjectHandle { self.instance2::<"CreateInstanceFrom", System::String, System::String, System::Runtime::Remoting::ObjectHandle>(a1, a2) }
    pub fn create_instance_from_and_unwrap(self, a1: System::String, a2: System::String) -> System::Object { self.instance2::<"CreateInstanceFromAndUnwrap", System::String, System::String, System::Object>(a1, a2) }
}
pub type AppDomainSetup =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AppDomainSetup">;
use super::*;
impl From<AppDomainSetup> for System::Object {
 fn from(v:AppDomainSetup)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,AppDomainSetup>(v)
}} 
impl AppDomainSetup {
    pub fn get_application_base(self) -> System::String { self.instance0::<"get_ApplicationBase", System::String>() }
    pub fn get_target_framework_name(self) -> System::String { self.instance0::<"get_TargetFrameworkName", System::String>() }
}
pub type AppDomainUnloadedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AppDomainUnloadedException">;
use super::*;
impl From<AppDomainUnloadedException> for System::SystemException {
 fn from(v:AppDomainUnloadedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,AppDomainUnloadedException>(v)
}} 
impl AppDomainUnloadedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ApplicationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ApplicationException">;
use super::*;
impl From<ApplicationException> for System::Exception {
 fn from(v:ApplicationException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,ApplicationException>(v)
}} 
impl ApplicationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ApplicationId =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ApplicationId">;
use super::*;
impl From<ApplicationId> for System::Object {
 fn from(v:ApplicationId)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,ApplicationId>(v)
}} 
impl ApplicationId {
    pub fn get_culture(self) -> System::String { self.instance0::<"get_Culture", System::String>() }
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_processor_architecture(self) -> System::String { self.instance0::<"get_ProcessorArchitecture", System::String>() }
    pub fn get_version(self) -> System::Version { self.instance0::<"get_Version", System::Version>() }
    pub fn copy(self) -> System::ApplicationId { self.instance0::<"Copy", System::ApplicationId>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
}
pub type ArgumentException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ArgumentException">;
use super::*;
impl From<ArgumentException> for System::SystemException {
 fn from(v:ArgumentException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ArgumentException>(v)
}} 
impl ArgumentException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_param_name(self) -> System::String { self.virt0::<"get_ParamName", System::String>() }
    pub fn throw_if_null_or_empty(a1: System::String, a2: System::String) { Self::static2::<"ThrowIfNullOrEmpty", System::String, System::String, ()>(a1, a2) }
    pub fn throw_if_null_or_white_space(a1: System::String, a2: System::String) { Self::static2::<"ThrowIfNullOrWhiteSpace", System::String, System::String, ()>(a1, a2) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ArgumentNullException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ArgumentNullException">;
use super::*;
impl From<ArgumentNullException> for System::ArgumentException {
 fn from(v:ArgumentNullException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,ArgumentNullException>(v)
}} 
impl ArgumentNullException {
    pub fn throw_if_null(a1: System::Object, a2: System::String) { Self::static2::<"ThrowIfNull", System::Object, System::String, ()>(a1, a2) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ArgumentOutOfRangeException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ArgumentOutOfRangeException">;
use super::*;
impl From<ArgumentOutOfRangeException> for System::ArgumentException {
 fn from(v:ArgumentOutOfRangeException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,ArgumentOutOfRangeException>(v)
}} 
impl ArgumentOutOfRangeException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_actual_value(self) -> System::Object { self.virt0::<"get_ActualValue", System::Object>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type ArithmeticException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ArithmeticException">;
use super::*;
impl From<ArithmeticException> for System::SystemException {
 fn from(v:ArithmeticException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ArithmeticException>(v)
}} 
impl ArithmeticException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ArrayTypeMismatchException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ArrayTypeMismatchException">;
use super::*;
impl From<ArrayTypeMismatchException> for System::SystemException {
 fn from(v:ArrayTypeMismatchException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ArrayTypeMismatchException>(v)
}} 
impl ArrayTypeMismatchException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type AssemblyLoadEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AssemblyLoadEventArgs">;
use super::*;
impl From<AssemblyLoadEventArgs> for System::EventArgs {
 fn from(v:AssemblyLoadEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,AssemblyLoadEventArgs>(v)
}} 
impl AssemblyLoadEventArgs {
    pub fn get_loaded_assembly(self) -> System::Reflection::Assembly { self.instance0::<"get_LoadedAssembly", System::Reflection::Assembly>() }
    pub fn new(a1: System::Reflection::Assembly) -> Self { Self::ctor1(a1) }
}
pub type AssemblyLoadEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AssemblyLoadEventHandler">;
use super::*;
impl From<AssemblyLoadEventHandler> for System::MulticastDelegate {
 fn from(v:AssemblyLoadEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,AssemblyLoadEventHandler>(v)
}} 
impl AssemblyLoadEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::AssemblyLoadEventArgs) { self.instance2::<"Invoke", System::Object, System::AssemblyLoadEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type AsyncCallback =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AsyncCallback">;
use super::*;
impl From<AsyncCallback> for System::MulticastDelegate {
 fn from(v:AsyncCallback)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,AsyncCallback>(v)
}} 
impl AsyncCallback {
    pub fn invoke(self, a1: System::IAsyncResult) { self.instance1::<"Invoke", System::IAsyncResult, ()>(a1) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type AttributeUsageAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.AttributeUsageAttribute">;
use super::*;
impl From<AttributeUsageAttribute> for System::Attribute {
 fn from(v:AttributeUsageAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,AttributeUsageAttribute>(v)
}} 
impl AttributeUsageAttribute {
    pub fn get_allow_multiple(self) -> bool { self.instance0::<"get_AllowMultiple", bool>() }
    pub fn set_allow_multiple(self, a1: bool) { self.instance1::<"set_AllowMultiple", bool, ()>(a1) }
    pub fn get_inherited(self) -> bool { self.instance0::<"get_Inherited", bool>() }
    pub fn set_inherited(self, a1: bool) { self.instance1::<"set_Inherited", bool, ()>(a1) }
}
pub type BitConverter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.BitConverter">;
use super::*;
impl From<BitConverter> for System::Object {
 fn from(v:BitConverter)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,BitConverter>(v)
}} 
impl BitConverter {
    pub fn double_to_int64_bits(a1: f64) -> i64 { Self::static1::<"DoubleToInt64Bits", f64, i64>(a1) }
    pub fn int64_bits_to_double(a1: i64) -> f64 { Self::static1::<"Int64BitsToDouble", i64, f64>(a1) }
    pub fn single_to_int32_bits(a1: f32) -> i32 { Self::static1::<"SingleToInt32Bits", f32, i32>(a1) }
    pub fn int32_bits_to_single(a1: i32) -> f32 { Self::static1::<"Int32BitsToSingle", i32, f32>(a1) }
    pub fn double_to_uint64_bits(a1: f64) -> u64 { Self::static1::<"DoubleToUInt64Bits", f64, u64>(a1) }
    pub fn uint64_bits_to_double(a1: u64) -> f64 { Self::static1::<"UInt64BitsToDouble", u64, f64>(a1) }
    pub fn single_to_uint32_bits(a1: f32) -> u32 { Self::static1::<"SingleToUInt32Bits", f32, u32>(a1) }
    pub fn uint32_bits_to_single(a1: u32) -> f32 { Self::static1::<"UInt32BitsToSingle", u32, f32>(a1) }
}
pub type CannotUnloadAppDomainException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CannotUnloadAppDomainException">;
use super::*;
impl From<CannotUnloadAppDomainException> for System::SystemException {
 fn from(v:CannotUnloadAppDomainException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,CannotUnloadAppDomainException>(v)
}} 
impl CannotUnloadAppDomainException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type CharEnumerator =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CharEnumerator">;
use super::*;
impl From<CharEnumerator> for System::Object {
 fn from(v:CharEnumerator)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,CharEnumerator>(v)
}} 
impl CharEnumerator {
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn move_next(self) -> bool { self.virt0::<"MoveNext", bool>() }
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
    pub fn reset(self) { self.virt0::<"Reset", ()>() }
}
pub type CLSCompliantAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CLSCompliantAttribute">;
use super::*;
impl From<CLSCompliantAttribute> for System::Attribute {
 fn from(v:CLSCompliantAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,CLSCompliantAttribute>(v)
}} 
impl CLSCompliantAttribute {
    pub fn get_is_compliant(self) -> bool { self.instance0::<"get_IsCompliant", bool>() }
    pub fn new(a1: bool) -> Self { Self::ctor1(a1) }
}
pub type ContextBoundObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ContextBoundObject">;
use super::*;
impl From<ContextBoundObject> for System::MarshalByRefObject {
 fn from(v:ContextBoundObject)->System::MarshalByRefObject{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MarshalByRefObject,ContextBoundObject>(v)
}} 
pub type ContextMarshalException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ContextMarshalException">;
use super::*;
impl From<ContextMarshalException> for System::SystemException {
 fn from(v:ContextMarshalException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ContextMarshalException>(v)
}} 
impl ContextMarshalException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ContextStaticAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ContextStaticAttribute">;
use super::*;
impl From<ContextStaticAttribute> for System::Attribute {
 fn from(v:ContextStaticAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ContextStaticAttribute>(v)
}} 
impl ContextStaticAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Convert =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Convert">;
use super::*;
impl From<Convert> for System::Object {
 fn from(v:Convert)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Convert>(v)
}} 
impl Convert {
    pub fn is_dbnull(a1: System::Object) -> bool { Self::static1::<"IsDBNull", System::Object, bool>(a1) }
    pub fn change_type(a1: System::Object, a2: System::Type) -> System::Object { Self::static2::<"ChangeType", System::Object, System::Type, System::Object>(a1, a2) }
    pub fn to_boolean(a1: System::Object) -> bool { Self::static1::<"ToBoolean", System::Object, bool>(a1) }
    pub fn to_sbyte(a1: System::Object) -> i8 { Self::static1::<"ToSByte", System::Object, i8>(a1) }
    pub fn to_byte(a1: System::Object) -> u8 { Self::static1::<"ToByte", System::Object, u8>(a1) }
    pub fn to_int16(a1: System::Object) -> i16 { Self::static1::<"ToInt16", System::Object, i16>(a1) }
    pub fn to_uint16(a1: System::Object) -> u16 { Self::static1::<"ToUInt16", System::Object, u16>(a1) }
    pub fn to_int32(a1: System::Object) -> i32 { Self::static1::<"ToInt32", System::Object, i32>(a1) }
    pub fn to_uint32(a1: System::Object) -> u32 { Self::static1::<"ToUInt32", System::Object, u32>(a1) }
    pub fn to_int64(a1: System::Object) -> i64 { Self::static1::<"ToInt64", System::Object, i64>(a1) }
    pub fn to_uint64(a1: System::Object) -> u64 { Self::static1::<"ToUInt64", System::Object, u64>(a1) }
    pub fn to_single(a1: System::Object) -> f32 { Self::static1::<"ToSingle", System::Object, f32>(a1) }
    pub fn to_double(a1: System::Object) -> f64 { Self::static1::<"ToDouble", System::Object, f64>(a1) }
    pub fn to_string(a1: System::Object) -> System::String { Self::static1::<"ToString", System::Object, System::String>(a1) }
}
pub type DataMisalignedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.DataMisalignedException">;
use super::*;
impl From<DataMisalignedException> for System::SystemException {
 fn from(v:DataMisalignedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,DataMisalignedException>(v)
}} 
impl DataMisalignedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DBNull =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.DBNull">;
use super::*;
impl From<DBNull> for System::Object {
 fn from(v:DBNull)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,DBNull>(v)
}} 
impl DBNull {
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type DivideByZeroException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.DivideByZeroException">;
use super::*;
impl From<DivideByZeroException> for System::ArithmeticException {
 fn from(v:DivideByZeroException)->System::ArithmeticException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArithmeticException,DivideByZeroException>(v)
}} 
impl DivideByZeroException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DllNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.DllNotFoundException">;
use super::*;
impl From<DllNotFoundException> for System::TypeLoadException {
 fn from(v:DllNotFoundException)->System::TypeLoadException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::TypeLoadException,DllNotFoundException>(v)
}} 
impl DllNotFoundException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type DuplicateWaitObjectException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.DuplicateWaitObjectException">;
use super::*;
impl From<DuplicateWaitObjectException> for System::ArgumentException {
 fn from(v:DuplicateWaitObjectException)->System::ArgumentException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArgumentException,DuplicateWaitObjectException>(v)
}} 
impl DuplicateWaitObjectException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EntryPointNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.EntryPointNotFoundException">;
use super::*;
impl From<EntryPointNotFoundException> for System::TypeLoadException {
 fn from(v:EntryPointNotFoundException)->System::TypeLoadException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::TypeLoadException,EntryPointNotFoundException>(v)
}} 
impl EntryPointNotFoundException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.EventArgs">;
use super::*;
impl From<EventArgs> for System::Object {
 fn from(v:EventArgs)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,EventArgs>(v)
}} 
impl EventArgs {
    pub fn new() -> Self { Self::ctor0() }
}
pub type EventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.EventHandler">;
use super::*;
impl From<EventHandler> for System::MulticastDelegate {
 fn from(v:EventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,EventHandler>(v)
}} 
impl EventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::EventArgs) { self.instance2::<"Invoke", System::Object, System::EventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ExecutionEngineException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ExecutionEngineException">;
use super::*;
impl From<ExecutionEngineException> for System::SystemException {
 fn from(v:ExecutionEngineException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,ExecutionEngineException>(v)
}} 
impl ExecutionEngineException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FieldAccessException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.FieldAccessException">;
use super::*;
impl From<FieldAccessException> for System::MemberAccessException {
 fn from(v:FieldAccessException)->System::MemberAccessException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MemberAccessException,FieldAccessException>(v)
}} 
impl FieldAccessException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FlagsAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.FlagsAttribute">;
use super::*;
impl From<FlagsAttribute> for System::Attribute {
 fn from(v:FlagsAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,FlagsAttribute>(v)
}} 
impl FlagsAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FormatException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.FormatException">;
use super::*;
impl From<FormatException> for System::SystemException {
 fn from(v:FormatException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,FormatException>(v)
}} 
impl FormatException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FormattableString =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.FormattableString">;
use super::*;
impl From<FormattableString> for System::Object {
 fn from(v:FormattableString)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,FormattableString>(v)
}} 
impl FormattableString {
    pub fn get_format(self) -> System::String { self.virt0::<"get_Format", System::String>() }
    pub fn get_argument_count(self) -> i32 { self.virt0::<"get_ArgumentCount", i32>() }
    pub fn invariant(a1: System::FormattableString) -> System::String { Self::static1::<"Invariant", System::FormattableString, System::String>(a1) }
    pub fn current_culture(a1: System::FormattableString) -> System::String { Self::static1::<"CurrentCulture", System::FormattableString, System::String>(a1) }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
}
pub type IAsyncDisposable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IAsyncDisposable">;
use super::*;
pub type IAsyncResult =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IAsyncResult">;
use super::*;
impl IAsyncResult {
    pub fn get_is_completed(self) -> bool { self.virt0::<"get_IsCompleted", bool>() }
    pub fn get_async_wait_handle(self) -> System::Threading::WaitHandle { self.virt0::<"get_AsyncWaitHandle", System::Threading::WaitHandle>() }
    pub fn get_async_state(self) -> System::Object { self.virt0::<"get_AsyncState", System::Object>() }
    pub fn get_completed_synchronously(self) -> bool { self.virt0::<"get_CompletedSynchronously", bool>() }
}
pub type ICloneable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ICloneable">;
use super::*;
impl ICloneable {
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
}
pub type IComparable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IComparable">;
use super::*;
pub type IConvertible =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IConvertible">;
use super::*;
pub type ICustomFormatter =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ICustomFormatter">;
use super::*;
pub type IDisposable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IDisposable">;
use super::*;
impl IDisposable {
    pub fn dispose(self) { self.virt0::<"Dispose", ()>() }
}
pub type IFormatProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IFormatProvider">;
use super::*;
pub type IFormattable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IFormattable">;
use super::*;
pub type IndexOutOfRangeException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IndexOutOfRangeException">;
use super::*;
impl From<IndexOutOfRangeException> for System::SystemException {
 fn from(v:IndexOutOfRangeException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,IndexOutOfRangeException>(v)
}} 
impl IndexOutOfRangeException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InsufficientExecutionStackException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InsufficientExecutionStackException">;
use super::*;
impl From<InsufficientExecutionStackException> for System::SystemException {
 fn from(v:InsufficientExecutionStackException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InsufficientExecutionStackException>(v)
}} 
impl InsufficientExecutionStackException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InsufficientMemoryException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InsufficientMemoryException">;
use super::*;
impl From<InsufficientMemoryException> for System::OutOfMemoryException {
 fn from(v:InsufficientMemoryException)->System::OutOfMemoryException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::OutOfMemoryException,InsufficientMemoryException>(v)
}} 
impl InsufficientMemoryException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidCastException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InvalidCastException">;
use super::*;
impl From<InvalidCastException> for System::SystemException {
 fn from(v:InvalidCastException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidCastException>(v)
}} 
impl InvalidCastException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidOperationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InvalidOperationException">;
use super::*;
impl From<InvalidOperationException> for System::SystemException {
 fn from(v:InvalidOperationException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidOperationException>(v)
}} 
impl InvalidOperationException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidProgramException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InvalidProgramException">;
use super::*;
impl From<InvalidProgramException> for System::SystemException {
 fn from(v:InvalidProgramException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,InvalidProgramException>(v)
}} 
impl InvalidProgramException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type InvalidTimeZoneException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.InvalidTimeZoneException">;
use super::*;
impl From<InvalidTimeZoneException> for System::Exception {
 fn from(v:InvalidTimeZoneException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,InvalidTimeZoneException>(v)
}} 
impl InvalidTimeZoneException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ISpanFormattable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ISpanFormattable">;
use super::*;
pub type IUtf8SpanFormattable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.IUtf8SpanFormattable">;
use super::*;
pub type LoaderOptimizationAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.LoaderOptimizationAttribute">;
use super::*;
impl From<LoaderOptimizationAttribute> for System::Attribute {
 fn from(v:LoaderOptimizationAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,LoaderOptimizationAttribute>(v)
}} 
impl LoaderOptimizationAttribute {
    pub fn new(a1: u8) -> Self { Self::ctor1(a1) }
}
pub type LocalDataStoreSlot =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.LocalDataStoreSlot">;
use super::*;
impl From<LocalDataStoreSlot> for System::Object {
 fn from(v:LocalDataStoreSlot)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,LocalDataStoreSlot>(v)
}} 
pub type MarshalByRefObject =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MarshalByRefObject">;
use super::*;
impl From<MarshalByRefObject> for System::Object {
 fn from(v:MarshalByRefObject)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MarshalByRefObject>(v)
}} 
impl MarshalByRefObject {
    pub fn get_lifetime_service(self) -> System::Object { self.instance0::<"GetLifetimeService", System::Object>() }
    pub fn initialize_lifetime_service(self) -> System::Object { self.virt0::<"InitializeLifetimeService", System::Object>() }
}
pub type MemberAccessException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MemberAccessException">;
use super::*;
impl From<MemberAccessException> for System::SystemException {
 fn from(v:MemberAccessException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,MemberAccessException>(v)
}} 
impl MemberAccessException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MemoryExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MemoryExtensions">;
use super::*;
impl From<MemoryExtensions> for System::Object {
 fn from(v:MemoryExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,MemoryExtensions>(v)
}} 
pub type MethodAccessException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MethodAccessException">;
use super::*;
impl From<MethodAccessException> for System::MemberAccessException {
 fn from(v:MethodAccessException)->System::MemberAccessException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MemberAccessException,MethodAccessException>(v)
}} 
impl MethodAccessException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MissingFieldException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MissingFieldException">;
use super::*;
impl From<MissingFieldException> for System::MissingMemberException {
 fn from(v:MissingFieldException)->System::MissingMemberException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MissingMemberException,MissingFieldException>(v)
}} 
impl MissingFieldException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type MissingMemberException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MissingMemberException">;
use super::*;
impl From<MissingMemberException> for System::MemberAccessException {
 fn from(v:MissingMemberException)->System::MemberAccessException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MemberAccessException,MissingMemberException>(v)
}} 
impl MissingMemberException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type MissingMethodException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MissingMethodException">;
use super::*;
impl From<MissingMethodException> for System::MissingMemberException {
 fn from(v:MissingMethodException)->System::MissingMemberException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MissingMemberException,MissingMethodException>(v)
}} 
impl MissingMethodException {
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type MulticastNotSupportedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MulticastNotSupportedException">;
use super::*;
impl From<MulticastNotSupportedException> for System::SystemException {
 fn from(v:MulticastNotSupportedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,MulticastNotSupportedException>(v)
}} 
impl MulticastNotSupportedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NonSerializedAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.NonSerializedAttribute">;
use super::*;
impl From<NonSerializedAttribute> for System::Attribute {
 fn from(v:NonSerializedAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,NonSerializedAttribute>(v)
}} 
impl NonSerializedAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NotFiniteNumberException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.NotFiniteNumberException">;
use super::*;
impl From<NotFiniteNumberException> for System::ArithmeticException {
 fn from(v:NotFiniteNumberException)->System::ArithmeticException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArithmeticException,NotFiniteNumberException>(v)
}} 
impl NotFiniteNumberException {
    pub fn get_offending_number(self) -> f64 { self.instance0::<"get_OffendingNumber", f64>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type NotImplementedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.NotImplementedException">;
use super::*;
impl From<NotImplementedException> for System::SystemException {
 fn from(v:NotImplementedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,NotImplementedException>(v)
}} 
impl NotImplementedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NotSupportedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.NotSupportedException">;
use super::*;
impl From<NotSupportedException> for System::SystemException {
 fn from(v:NotSupportedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,NotSupportedException>(v)
}} 
impl NotSupportedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Nullable =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Nullable">;
use super::*;
impl From<Nullable> for System::Object {
 fn from(v:Nullable)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Nullable>(v)
}} 
impl Nullable {
    pub fn get_underlying_type(a1: System::Type) -> System::Type { Self::static1::<"GetUnderlyingType", System::Type, System::Type>(a1) }
}
pub type NullReferenceException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.NullReferenceException">;
use super::*;
impl From<NullReferenceException> for System::SystemException {
 fn from(v:NullReferenceException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,NullReferenceException>(v)
}} 
impl NullReferenceException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ObjectDisposedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ObjectDisposedException">;
use super::*;
impl From<ObjectDisposedException> for System::InvalidOperationException {
 fn from(v:ObjectDisposedException)->System::InvalidOperationException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::InvalidOperationException,ObjectDisposedException>(v)
}} 
impl ObjectDisposedException {
    pub fn throw_if(a1: bool, a2: System::Object) { Self::static2::<"ThrowIf", bool, System::Object, ()>(a1, a2) }
    pub fn get_message(self) -> System::String { self.virt0::<"get_Message", System::String>() }
    pub fn get_object_name(self) -> System::String { self.instance0::<"get_ObjectName", System::String>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ObsoleteAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ObsoleteAttribute">;
use super::*;
impl From<ObsoleteAttribute> for System::Attribute {
 fn from(v:ObsoleteAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ObsoleteAttribute>(v)
}} 
impl ObsoleteAttribute {
    pub fn get_message(self) -> System::String { self.instance0::<"get_Message", System::String>() }
    pub fn get_is_error(self) -> bool { self.instance0::<"get_IsError", bool>() }
    pub fn get_diagnostic_id(self) -> System::String { self.instance0::<"get_DiagnosticId", System::String>() }
    pub fn set_diagnostic_id(self, a1: System::String) { self.instance1::<"set_DiagnosticId", System::String, ()>(a1) }
    pub fn get_url_format(self) -> System::String { self.instance0::<"get_UrlFormat", System::String>() }
    pub fn set_url_format(self, a1: System::String) { self.instance1::<"set_UrlFormat", System::String, ()>(a1) }
    pub fn new() -> Self { Self::ctor0() }
}
pub type OperatingSystem =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.OperatingSystem">;
use super::*;
impl From<OperatingSystem> for System::Object {
 fn from(v:OperatingSystem)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,OperatingSystem>(v)
}} 
impl OperatingSystem {
    pub fn get_service_pack(self) -> System::String { self.instance0::<"get_ServicePack", System::String>() }
    pub fn get_version(self) -> System::Version { self.instance0::<"get_Version", System::Version>() }
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_version_string(self) -> System::String { self.instance0::<"get_VersionString", System::String>() }
    pub fn is_osplatform(a1: System::String) -> bool { Self::static1::<"IsOSPlatform", System::String, bool>(a1) }
    pub fn is_browser() -> bool { Self::static0::<"IsBrowser", bool>() }
    pub fn is_wasi() -> bool { Self::static0::<"IsWasi", bool>() }
    pub fn is_linux() -> bool { Self::static0::<"IsLinux", bool>() }
    pub fn is_free_bsd() -> bool { Self::static0::<"IsFreeBSD", bool>() }
    pub fn is_android() -> bool { Self::static0::<"IsAndroid", bool>() }
    pub fn is_ios() -> bool { Self::static0::<"IsIOS", bool>() }
    pub fn is_mac_os() -> bool { Self::static0::<"IsMacOS", bool>() }
    pub fn is_mac_catalyst() -> bool { Self::static0::<"IsMacCatalyst", bool>() }
    pub fn is_tv_os() -> bool { Self::static0::<"IsTvOS", bool>() }
    pub fn is_watch_os() -> bool { Self::static0::<"IsWatchOS", bool>() }
    pub fn is_windows() -> bool { Self::static0::<"IsWindows", bool>() }
}
pub type OperationCanceledException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.OperationCanceledException">;
use super::*;
impl From<OperationCanceledException> for System::SystemException {
 fn from(v:OperationCanceledException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,OperationCanceledException>(v)
}} 
impl OperationCanceledException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OutOfMemoryException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.OutOfMemoryException">;
use super::*;
impl From<OutOfMemoryException> for System::SystemException {
 fn from(v:OutOfMemoryException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,OutOfMemoryException>(v)
}} 
impl OutOfMemoryException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type OverflowException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.OverflowException">;
use super::*;
impl From<OverflowException> for System::ArithmeticException {
 fn from(v:OverflowException)->System::ArithmeticException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::ArithmeticException,OverflowException>(v)
}} 
impl OverflowException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ParamArrayAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ParamArrayAttribute">;
use super::*;
impl From<ParamArrayAttribute> for System::Attribute {
 fn from(v:ParamArrayAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ParamArrayAttribute>(v)
}} 
impl ParamArrayAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type PlatformNotSupportedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.PlatformNotSupportedException">;
use super::*;
impl From<PlatformNotSupportedException> for System::NotSupportedException {
 fn from(v:PlatformNotSupportedException)->System::NotSupportedException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::NotSupportedException,PlatformNotSupportedException>(v)
}} 
impl PlatformNotSupportedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Random =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Random">;
use super::*;
impl From<Random> for System::Object {
 fn from(v:Random)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Random>(v)
}} 
impl Random {
    pub fn get_shared() -> System::Random { Self::static0::<"get_Shared", System::Random>() }
    pub fn next(self) -> i32 { self.virt0::<"Next", i32>() }
    pub fn next_int64(self) -> i64 { self.virt0::<"NextInt64", i64>() }
    pub fn next_single(self) -> f32 { self.virt0::<"NextSingle", f32>() }
    pub fn next_double(self) -> f64 { self.virt0::<"NextDouble", f64>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type RankException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.RankException">;
use super::*;
impl From<RankException> for System::SystemException {
 fn from(v:RankException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,RankException>(v)
}} 
impl RankException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ResolveEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ResolveEventArgs">;
use super::*;
impl From<ResolveEventArgs> for System::EventArgs {
 fn from(v:ResolveEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,ResolveEventArgs>(v)
}} 
impl ResolveEventArgs {
    pub fn get_name(self) -> System::String { self.instance0::<"get_Name", System::String>() }
    pub fn get_requesting_assembly(self) -> System::Reflection::Assembly { self.instance0::<"get_RequestingAssembly", System::Reflection::Assembly>() }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type ResolveEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ResolveEventHandler">;
use super::*;
impl From<ResolveEventHandler> for System::MulticastDelegate {
 fn from(v:ResolveEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ResolveEventHandler>(v)
}} 
impl ResolveEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::ResolveEventArgs) -> System::Reflection::Assembly { self.instance2::<"Invoke", System::Object, System::ResolveEventArgs, System::Reflection::Assembly>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) -> System::Reflection::Assembly { self.instance1::<"EndInvoke", System::IAsyncResult, System::Reflection::Assembly>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type SerializableAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.SerializableAttribute">;
use super::*;
impl From<SerializableAttribute> for System::Attribute {
 fn from(v:SerializableAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,SerializableAttribute>(v)
}} 
impl SerializableAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type StackOverflowException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.StackOverflowException">;
use super::*;
impl From<StackOverflowException> for System::SystemException {
 fn from(v:StackOverflowException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,StackOverflowException>(v)
}} 
impl StackOverflowException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type StringComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.StringComparer">;
use super::*;
impl From<StringComparer> for System::Object {
 fn from(v:StringComparer)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringComparer>(v)
}} 
impl StringComparer {
    pub fn get_invariant_culture() -> System::StringComparer { Self::static0::<"get_InvariantCulture", System::StringComparer>() }
    pub fn get_invariant_culture_ignore_case() -> System::StringComparer { Self::static0::<"get_InvariantCultureIgnoreCase", System::StringComparer>() }
    pub fn get_current_culture() -> System::StringComparer { Self::static0::<"get_CurrentCulture", System::StringComparer>() }
    pub fn get_current_culture_ignore_case() -> System::StringComparer { Self::static0::<"get_CurrentCultureIgnoreCase", System::StringComparer>() }
    pub fn get_ordinal() -> System::StringComparer { Self::static0::<"get_Ordinal", System::StringComparer>() }
    pub fn get_ordinal_ignore_case() -> System::StringComparer { Self::static0::<"get_OrdinalIgnoreCase", System::StringComparer>() }
    pub fn create(a1: System::Globalization::CultureInfo, a2: bool) -> System::StringComparer { Self::static2::<"Create", System::Globalization::CultureInfo, bool, System::StringComparer>(a1, a2) }
    pub fn compare(self, a1: System::Object, a2: System::Object) -> i32 { self.instance2::<"Compare", System::Object, System::Object, i32>(a1, a2) }
    pub fn equals(self, a1: System::Object, a2: System::Object) -> bool { self.instance2::<"Equals", System::Object, System::Object, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: System::Object) -> i32 { self.instance1::<"GetHashCode", System::Object, i32>(a1) }
}
pub type CultureAwareComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.CultureAwareComparer">;
use super::*;
impl From<CultureAwareComparer> for System::StringComparer {
 fn from(v:CultureAwareComparer)->System::StringComparer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::StringComparer,CultureAwareComparer>(v)
}} 
impl CultureAwareComparer {
    pub fn compare(self, a1: System::String, a2: System::String) -> i32 { self.instance2::<"Compare", System::String, System::String, i32>(a1, a2) }
    pub fn equals(self, a1: System::String, a2: System::String) -> bool { self.instance2::<"Equals", System::String, System::String, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: System::String) -> i32 { self.instance1::<"GetHashCode", System::String, i32>(a1) }
}
pub type OrdinalComparer =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.OrdinalComparer">;
use super::*;
impl From<OrdinalComparer> for System::StringComparer {
 fn from(v:OrdinalComparer)->System::StringComparer{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::StringComparer,OrdinalComparer>(v)
}} 
impl OrdinalComparer {
    pub fn compare(self, a1: System::String, a2: System::String) -> i32 { self.instance2::<"Compare", System::String, System::String, i32>(a1, a2) }
    pub fn equals(self, a1: System::String, a2: System::String) -> bool { self.instance2::<"Equals", System::String, System::String, bool>(a1, a2) }
    pub fn get_hash_code(self, a1: System::String) -> i32 { self.instance1::<"GetHashCode", System::String, i32>(a1) }
}
pub type StringNormalizationExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.StringNormalizationExtensions">;
use super::*;
impl From<StringNormalizationExtensions> for System::Object {
 fn from(v:StringNormalizationExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,StringNormalizationExtensions>(v)
}} 
impl StringNormalizationExtensions {
    pub fn is_normalized(a1: System::String) -> bool { Self::static1::<"IsNormalized", System::String, bool>(a1) }
    pub fn normalize(a1: System::String) -> System::String { Self::static1::<"Normalize", System::String, System::String>(a1) }
}
pub type SystemException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.SystemException">;
use super::*;
impl From<SystemException> for System::Exception {
 fn from(v:SystemException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,SystemException>(v)
}} 
impl SystemException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type STAThreadAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.STAThreadAttribute">;
use super::*;
impl From<STAThreadAttribute> for System::Attribute {
 fn from(v:STAThreadAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,STAThreadAttribute>(v)
}} 
impl STAThreadAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type MTAThreadAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.MTAThreadAttribute">;
use super::*;
impl From<MTAThreadAttribute> for System::Attribute {
 fn from(v:MTAThreadAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,MTAThreadAttribute>(v)
}} 
impl MTAThreadAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type ThreadStaticAttribute =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.ThreadStaticAttribute">;
use super::*;
impl From<ThreadStaticAttribute> for System::Attribute {
 fn from(v:ThreadStaticAttribute)->System::Attribute{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Attribute,ThreadStaticAttribute>(v)
}} 
impl ThreadStaticAttribute {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TimeoutException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TimeoutException">;
use super::*;
impl From<TimeoutException> for System::SystemException {
 fn from(v:TimeoutException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,TimeoutException>(v)
}} 
impl TimeoutException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TimeZone =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TimeZone">;
use super::*;
impl From<TimeZone> for System::Object {
 fn from(v:TimeZone)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TimeZone>(v)
}} 
impl TimeZone {
    pub fn get_current_time_zone() -> System::TimeZone { Self::static0::<"get_CurrentTimeZone", System::TimeZone>() }
    pub fn get_standard_name(self) -> System::String { self.virt0::<"get_StandardName", System::String>() }
    pub fn get_daylight_name(self) -> System::String { self.virt0::<"get_DaylightName", System::String>() }
}
pub type TimeZoneInfo =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TimeZoneInfo">;
use super::*;
impl From<TimeZoneInfo> for System::Object {
 fn from(v:TimeZoneInfo)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TimeZoneInfo>(v)
}} 
impl TimeZoneInfo {
    pub fn get_id(self) -> System::String { self.instance0::<"get_Id", System::String>() }
    pub fn get_has_iana_id(self) -> bool { self.instance0::<"get_HasIanaId", bool>() }
    pub fn get_display_name(self) -> System::String { self.instance0::<"get_DisplayName", System::String>() }
    pub fn get_standard_name(self) -> System::String { self.instance0::<"get_StandardName", System::String>() }
    pub fn get_daylight_name(self) -> System::String { self.instance0::<"get_DaylightName", System::String>() }
    pub fn get_supports_daylight_saving_time(self) -> bool { self.instance0::<"get_SupportsDaylightSavingTime", bool>() }
    pub fn clear_cached_data() { Self::static0::<"ClearCachedData", ()>() }
    pub fn find_system_time_zone_by_id(a1: System::String) -> System::TimeZoneInfo { Self::static1::<"FindSystemTimeZoneById", System::String, System::TimeZoneInfo>(a1) }
    pub fn equals(self, a1: System::TimeZoneInfo) -> bool { self.instance1::<"Equals", System::TimeZoneInfo, bool>(a1) }
    pub fn from_serialized_string(a1: System::String) -> System::TimeZoneInfo { Self::static1::<"FromSerializedString", System::String, System::TimeZoneInfo>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn has_same_rules(self, a1: System::TimeZoneInfo) -> bool { self.instance1::<"HasSameRules", System::TimeZoneInfo, bool>(a1) }
    pub fn get_local() -> System::TimeZoneInfo { Self::static0::<"get_Local", System::TimeZoneInfo>() }
    pub fn to_serialized_string(self) -> System::String { self.instance0::<"ToSerializedString", System::String>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn get_utc() -> System::TimeZoneInfo { Self::static0::<"get_Utc", System::TimeZoneInfo>() }
}
pub type TimeZoneNotFoundException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TimeZoneNotFoundException">;
use super::*;
impl From<TimeZoneNotFoundException> for System::Exception {
 fn from(v:TimeZoneNotFoundException)->System::Exception{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Exception,TimeZoneNotFoundException>(v)
}} 
impl TimeZoneNotFoundException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type Tuple =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Tuple">;
use super::*;
impl From<Tuple> for System::Object {
 fn from(v:Tuple)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Tuple>(v)
}} 
pub type TupleExtensions =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TupleExtensions">;
use super::*;
impl From<TupleExtensions> for System::Object {
 fn from(v:TupleExtensions)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TupleExtensions>(v)
}} 
pub type TypeAccessException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TypeAccessException">;
use super::*;
impl From<TypeAccessException> for System::TypeLoadException {
 fn from(v:TypeAccessException)->System::TypeLoadException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::TypeLoadException,TypeAccessException>(v)
}} 
impl TypeAccessException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type TypeInitializationException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TypeInitializationException">;
use super::*;
impl From<TypeInitializationException> for System::SystemException {
 fn from(v:TypeInitializationException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,TypeInitializationException>(v)
}} 
impl TypeInitializationException {
    pub fn get_type_name(self) -> System::String { self.instance0::<"get_TypeName", System::String>() }
    pub fn new(a1: System::String, a2: System::Exception) -> Self { Self::ctor2(a1, a2) }
}
pub type TypeUnloadedException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TypeUnloadedException">;
use super::*;
impl From<TypeUnloadedException> for System::SystemException {
 fn from(v:TypeUnloadedException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,TypeUnloadedException>(v)
}} 
impl TypeUnloadedException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnauthorizedAccessException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.UnauthorizedAccessException">;
use super::*;
impl From<UnauthorizedAccessException> for System::SystemException {
 fn from(v:UnauthorizedAccessException)->System::SystemException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::SystemException,UnauthorizedAccessException>(v)
}} 
impl UnauthorizedAccessException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UnhandledExceptionEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.UnhandledExceptionEventArgs">;
use super::*;
impl From<UnhandledExceptionEventArgs> for System::EventArgs {
 fn from(v:UnhandledExceptionEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,UnhandledExceptionEventArgs>(v)
}} 
impl UnhandledExceptionEventArgs {
    pub fn get_exception_object(self) -> System::Object { self.instance0::<"get_ExceptionObject", System::Object>() }
    pub fn get_is_terminating(self) -> bool { self.instance0::<"get_IsTerminating", bool>() }
    pub fn new(a1: System::Object, a2: bool) -> Self { Self::ctor2(a1, a2) }
}
pub type UnhandledExceptionEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.UnhandledExceptionEventHandler">;
use super::*;
impl From<UnhandledExceptionEventHandler> for System::MulticastDelegate {
 fn from(v:UnhandledExceptionEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,UnhandledExceptionEventHandler>(v)
}} 
impl UnhandledExceptionEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::UnhandledExceptionEventArgs) { self.instance2::<"Invoke", System::Object, System::UnhandledExceptionEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type UnitySerializationHolder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.UnitySerializationHolder">;
use super::*;
impl From<UnitySerializationHolder> for System::Object {
 fn from(v:UnitySerializationHolder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,UnitySerializationHolder>(v)
}} 
pub type Version =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.Version">;
use super::*;
impl From<Version> for System::Object {
 fn from(v:Version)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Version>(v)
}} 
impl Version {
    pub fn clone(self) -> System::Object { self.virt0::<"Clone", System::Object>() }
    pub fn get_major(self) -> i32 { self.instance0::<"get_Major", i32>() }
    pub fn get_minor(self) -> i32 { self.instance0::<"get_Minor", i32>() }
    pub fn get_build(self) -> i32 { self.instance0::<"get_Build", i32>() }
    pub fn get_revision(self) -> i32 { self.instance0::<"get_Revision", i32>() }
    pub fn get_major_revision(self) -> i16 { self.instance0::<"get_MajorRevision", i16>() }
    pub fn get_minor_revision(self) -> i16 { self.instance0::<"get_MinorRevision", i16>() }
    pub fn compare_to(self, a1: System::Object) -> i32 { self.instance1::<"CompareTo", System::Object, i32>(a1) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn parse(a1: System::String) -> System::Version { Self::static1::<"Parse", System::String, System::Version>(a1) }
    pub fn op_equality(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_Equality", System::Version, System::Version, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_Inequality", System::Version, System::Version, bool>(a1, a2) }
    pub fn op_less_than(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_LessThan", System::Version, System::Version, bool>(a1, a2) }
    pub fn op_less_than_or_equal(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_LessThanOrEqual", System::Version, System::Version, bool>(a1, a2) }
    pub fn op_greater_than(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_GreaterThan", System::Version, System::Version, bool>(a1, a2) }
    pub fn op_greater_than_or_equal(a1: System::Version, a2: System::Version) -> bool { Self::static2::<"op_GreaterThanOrEqual", System::Version, System::Version, bool>(a1, a2) }
    pub fn new(a1: i32, a2: i32, a3: i32) -> Self { Self::ctor3(a1, a2, a3) }
}
pub type WeakReference =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.WeakReference">;
use super::*;
impl From<WeakReference> for System::Object {
 fn from(v:WeakReference)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,WeakReference>(v)
}} 
impl WeakReference {
    pub fn get_track_resurrection(self) -> bool { self.virt0::<"get_TrackResurrection", bool>() }
    pub fn get_is_alive(self) -> bool { self.virt0::<"get_IsAlive", bool>() }
    pub fn get_target(self) -> System::Object { self.virt0::<"get_Target", System::Object>() }
    pub fn set_target(self, a1: System::Object) { self.instance1::<"set_Target", System::Object, ()>(a1) }
    pub fn new(a1: System::Object) -> Self { Self::ctor1(a1) }
}
pub type TimeProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","System.TimeProvider">;
use super::*;
impl From<TimeProvider> for System::Object {
 fn from(v:TimeProvider)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,TimeProvider>(v)
}} 
impl TimeProvider {
    pub fn get_system() -> System::TimeProvider { Self::static0::<"get_System", System::TimeProvider>() }
    pub fn get_local_time_zone(self) -> System::TimeZoneInfo { self.virt0::<"get_LocalTimeZone", System::TimeZoneInfo>() }
    pub fn get_timestamp_frequency(self) -> i64 { self.virt0::<"get_TimestampFrequency", i64>() }
    pub fn get_timestamp(self) -> i64 { self.virt0::<"GetTimestamp", i64>() }
}
pub type Console =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Console","System.Console">;
use super::*;
impl From<Console> for System::Object {
 fn from(v:Console)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Console>(v)
}} 
impl Console {
    pub fn get_in() -> System::IO::TextReader { Self::static0::<"get_In", System::IO::TextReader>() }
    pub fn get_input_encoding() -> System::Text::Encoding { Self::static0::<"get_InputEncoding", System::Text::Encoding>() }
    pub fn set_input_encoding(a1: System::Text::Encoding) { Self::static1::<"set_InputEncoding", System::Text::Encoding, ()>(a1) }
    pub fn get_output_encoding() -> System::Text::Encoding { Self::static0::<"get_OutputEncoding", System::Text::Encoding>() }
    pub fn set_output_encoding(a1: System::Text::Encoding) { Self::static1::<"set_OutputEncoding", System::Text::Encoding, ()>(a1) }
    pub fn get_key_available() -> bool { Self::static0::<"get_KeyAvailable", bool>() }
    pub fn get_out() -> System::IO::TextWriter { Self::static0::<"get_Out", System::IO::TextWriter>() }
    pub fn get_error() -> System::IO::TextWriter { Self::static0::<"get_Error", System::IO::TextWriter>() }
    pub fn get_is_input_redirected() -> bool { Self::static0::<"get_IsInputRedirected", bool>() }
    pub fn get_is_output_redirected() -> bool { Self::static0::<"get_IsOutputRedirected", bool>() }
    pub fn get_is_error_redirected() -> bool { Self::static0::<"get_IsErrorRedirected", bool>() }
    pub fn get_cursor_size() -> i32 { Self::static0::<"get_CursorSize", i32>() }
    pub fn set_cursor_size(a1: i32) { Self::static1::<"set_CursorSize", i32, ()>(a1) }
    pub fn get_number_lock() -> bool { Self::static0::<"get_NumberLock", bool>() }
    pub fn get_caps_lock() -> bool { Self::static0::<"get_CapsLock", bool>() }
    pub fn reset_color() { Self::static0::<"ResetColor", ()>() }
    pub fn get_buffer_width() -> i32 { Self::static0::<"get_BufferWidth", i32>() }
    pub fn set_buffer_width(a1: i32) { Self::static1::<"set_BufferWidth", i32, ()>(a1) }
    pub fn get_buffer_height() -> i32 { Self::static0::<"get_BufferHeight", i32>() }
    pub fn set_buffer_height(a1: i32) { Self::static1::<"set_BufferHeight", i32, ()>(a1) }
    pub fn set_buffer_size(a1: i32, a2: i32) { Self::static2::<"SetBufferSize", i32, i32, ()>(a1, a2) }
    pub fn get_window_left() -> i32 { Self::static0::<"get_WindowLeft", i32>() }
    pub fn set_window_left(a1: i32) { Self::static1::<"set_WindowLeft", i32, ()>(a1) }
    pub fn get_window_top() -> i32 { Self::static0::<"get_WindowTop", i32>() }
    pub fn set_window_top(a1: i32) { Self::static1::<"set_WindowTop", i32, ()>(a1) }
    pub fn get_window_width() -> i32 { Self::static0::<"get_WindowWidth", i32>() }
    pub fn set_window_width(a1: i32) { Self::static1::<"set_WindowWidth", i32, ()>(a1) }
    pub fn get_window_height() -> i32 { Self::static0::<"get_WindowHeight", i32>() }
    pub fn set_window_height(a1: i32) { Self::static1::<"set_WindowHeight", i32, ()>(a1) }
    pub fn set_window_position(a1: i32, a2: i32) { Self::static2::<"SetWindowPosition", i32, i32, ()>(a1, a2) }
    pub fn set_window_size(a1: i32, a2: i32) { Self::static2::<"SetWindowSize", i32, i32, ()>(a1, a2) }
    pub fn get_largest_window_width() -> i32 { Self::static0::<"get_LargestWindowWidth", i32>() }
    pub fn get_largest_window_height() -> i32 { Self::static0::<"get_LargestWindowHeight", i32>() }
    pub fn get_cursor_visible() -> bool { Self::static0::<"get_CursorVisible", bool>() }
    pub fn set_cursor_visible(a1: bool) { Self::static1::<"set_CursorVisible", bool, ()>(a1) }
    pub fn get_cursor_left() -> i32 { Self::static0::<"get_CursorLeft", i32>() }
    pub fn set_cursor_left(a1: i32) { Self::static1::<"set_CursorLeft", i32, ()>(a1) }
    pub fn get_cursor_top() -> i32 { Self::static0::<"get_CursorTop", i32>() }
    pub fn set_cursor_top(a1: i32) { Self::static1::<"set_CursorTop", i32, ()>(a1) }
    pub fn get_title() -> System::String { Self::static0::<"get_Title", System::String>() }
    pub fn set_title(a1: System::String) { Self::static1::<"set_Title", System::String, ()>(a1) }
    pub fn beep() { Self::static0::<"Beep", ()>() }
    pub fn clear() { Self::static0::<"Clear", ()>() }
    pub fn set_cursor_position(a1: i32, a2: i32) { Self::static2::<"SetCursorPosition", i32, i32, ()>(a1, a2) }
    pub fn add_cancel_key_press(a1: System::ConsoleCancelEventHandler) { Self::static1::<"add_CancelKeyPress", System::ConsoleCancelEventHandler, ()>(a1) }
    pub fn remove_cancel_key_press(a1: System::ConsoleCancelEventHandler) { Self::static1::<"remove_CancelKeyPress", System::ConsoleCancelEventHandler, ()>(a1) }
    pub fn get_treat_control_cas_input() -> bool { Self::static0::<"get_TreatControlCAsInput", bool>() }
    pub fn set_treat_control_cas_input(a1: bool) { Self::static1::<"set_TreatControlCAsInput", bool, ()>(a1) }
    pub fn open_standard_input() -> System::IO::Stream { Self::static0::<"OpenStandardInput", System::IO::Stream>() }
    pub fn open_standard_output() -> System::IO::Stream { Self::static0::<"OpenStandardOutput", System::IO::Stream>() }
    pub fn open_standard_error() -> System::IO::Stream { Self::static0::<"OpenStandardError", System::IO::Stream>() }
    pub fn set_in(a1: System::IO::TextReader) { Self::static1::<"SetIn", System::IO::TextReader, ()>(a1) }
    pub fn set_out(a1: System::IO::TextWriter) { Self::static1::<"SetOut", System::IO::TextWriter, ()>(a1) }
    pub fn set_error(a1: System::IO::TextWriter) { Self::static1::<"SetError", System::IO::TextWriter, ()>(a1) }
    pub fn read() -> i32 { Self::static0::<"Read", i32>() }
    pub fn read_line() -> System::String { Self::static0::<"ReadLine", System::String>() }
    pub fn write_line() { Self::static0::<"WriteLine", ()>() }
    pub fn write(a1: System::String, a2: System::Object) { Self::static2::<"Write", System::String, System::Object, ()>(a1, a2) }
}
pub type ConsoleCancelEventHandler =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Console","System.ConsoleCancelEventHandler">;
use super::*;
impl From<ConsoleCancelEventHandler> for System::MulticastDelegate {
 fn from(v:ConsoleCancelEventHandler)->System::MulticastDelegate{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::MulticastDelegate,ConsoleCancelEventHandler>(v)
}} 
impl ConsoleCancelEventHandler {
    pub fn invoke(self, a1: System::Object, a2: System::ConsoleCancelEventArgs) { self.instance2::<"Invoke", System::Object, System::ConsoleCancelEventArgs, ()>(a1, a2) }
    pub fn end_invoke(self, a1: System::IAsyncResult) { self.instance1::<"EndInvoke", System::IAsyncResult, ()>(a1) }
    pub fn new(a1: System::Object, a2: isize) -> Self { Self::ctor2(a1, a2) }
}
pub type ConsoleCancelEventArgs =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Console","System.ConsoleCancelEventArgs">;
use super::*;
impl From<ConsoleCancelEventArgs> for System::EventArgs {
 fn from(v:ConsoleCancelEventArgs)->System::EventArgs{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::EventArgs,ConsoleCancelEventArgs>(v)
}} 
impl ConsoleCancelEventArgs {
    pub fn get_cancel(self) -> bool { self.instance0::<"get_Cancel", bool>() }
    pub fn set_cancel(self, a1: bool) { self.instance1::<"set_Cancel", bool, ()>(a1) }
}
pub type IServiceProvider =  crate::intrinsics::RustcCLRInteropManagedClass<"System.ComponentModel","System.IServiceProvider">;
use super::*;
pub type GenericUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.GenericUriParser">;
use super::*;
impl From<GenericUriParser> for System::UriParser {
 fn from(v:GenericUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,GenericUriParser>(v)
}} 
pub type Uri =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.Uri">;
use super::*;
impl From<Uri> for System::Object {
 fn from(v:Uri)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Uri>(v)
}} 
impl Uri {
    pub fn get_absolute_path(self) -> System::String { self.instance0::<"get_AbsolutePath", System::String>() }
    pub fn get_absolute_uri(self) -> System::String { self.instance0::<"get_AbsoluteUri", System::String>() }
    pub fn get_local_path(self) -> System::String { self.instance0::<"get_LocalPath", System::String>() }
    pub fn get_authority(self) -> System::String { self.instance0::<"get_Authority", System::String>() }
    pub fn get_is_default_port(self) -> bool { self.instance0::<"get_IsDefaultPort", bool>() }
    pub fn get_is_file(self) -> bool { self.instance0::<"get_IsFile", bool>() }
    pub fn get_is_loopback(self) -> bool { self.instance0::<"get_IsLoopback", bool>() }
    pub fn get_path_and_query(self) -> System::String { self.instance0::<"get_PathAndQuery", System::String>() }
    pub fn get_is_unc(self) -> bool { self.instance0::<"get_IsUnc", bool>() }
    pub fn get_host(self) -> System::String { self.instance0::<"get_Host", System::String>() }
    pub fn get_port(self) -> i32 { self.instance0::<"get_Port", i32>() }
    pub fn get_query(self) -> System::String { self.instance0::<"get_Query", System::String>() }
    pub fn get_fragment(self) -> System::String { self.instance0::<"get_Fragment", System::String>() }
    pub fn get_scheme(self) -> System::String { self.instance0::<"get_Scheme", System::String>() }
    pub fn get_original_string(self) -> System::String { self.instance0::<"get_OriginalString", System::String>() }
    pub fn get_dns_safe_host(self) -> System::String { self.instance0::<"get_DnsSafeHost", System::String>() }
    pub fn get_idn_host(self) -> System::String { self.instance0::<"get_IdnHost", System::String>() }
    pub fn get_is_absolute_uri(self) -> bool { self.instance0::<"get_IsAbsoluteUri", bool>() }
    pub fn get_user_escaped(self) -> bool { self.instance0::<"get_UserEscaped", bool>() }
    pub fn get_user_info(self) -> System::String { self.instance0::<"get_UserInfo", System::String>() }
    pub fn is_hex_encoding(a1: System::String, a2: i32) -> bool { Self::static2::<"IsHexEncoding", System::String, i32, bool>(a1, a2) }
    pub fn check_scheme_name(a1: System::String) -> bool { Self::static1::<"CheckSchemeName", System::String, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn op_equality(a1: System::Uri, a2: System::Uri) -> bool { Self::static2::<"op_Equality", System::Uri, System::Uri, bool>(a1, a2) }
    pub fn op_inequality(a1: System::Uri, a2: System::Uri) -> bool { Self::static2::<"op_Inequality", System::Uri, System::Uri, bool>(a1, a2) }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn make_relative_uri(self, a1: System::Uri) -> System::Uri { self.instance1::<"MakeRelativeUri", System::Uri, System::Uri>(a1) }
    pub fn make_relative(self, a1: System::Uri) -> System::String { self.instance1::<"MakeRelative", System::Uri, System::String>(a1) }
    pub fn is_well_formed_original_string(self) -> bool { self.instance0::<"IsWellFormedOriginalString", bool>() }
    pub fn unescape_data_string(a1: System::String) -> System::String { Self::static1::<"UnescapeDataString", System::String, System::String>(a1) }
    pub fn escape_uri_string(a1: System::String) -> System::String { Self::static1::<"EscapeUriString", System::String, System::String>(a1) }
    pub fn escape_data_string(a1: System::String) -> System::String { Self::static1::<"EscapeDataString", System::String, System::String>(a1) }
    pub fn is_base_of(self, a1: System::Uri) -> bool { self.instance1::<"IsBaseOf", System::Uri, bool>(a1) }
    pub fn new(a1: System::String) -> Self { Self::ctor1(a1) }
}
pub type UriBuilder =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.UriBuilder">;
use super::*;
impl From<UriBuilder> for System::Object {
 fn from(v:UriBuilder)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,UriBuilder>(v)
}} 
impl UriBuilder {
    pub fn get_scheme(self) -> System::String { self.instance0::<"get_Scheme", System::String>() }
    pub fn set_scheme(self, a1: System::String) { self.instance1::<"set_Scheme", System::String, ()>(a1) }
    pub fn get_user_name(self) -> System::String { self.instance0::<"get_UserName", System::String>() }
    pub fn set_user_name(self, a1: System::String) { self.instance1::<"set_UserName", System::String, ()>(a1) }
    pub fn get_password(self) -> System::String { self.instance0::<"get_Password", System::String>() }
    pub fn set_password(self, a1: System::String) { self.instance1::<"set_Password", System::String, ()>(a1) }
    pub fn get_host(self) -> System::String { self.instance0::<"get_Host", System::String>() }
    pub fn set_host(self, a1: System::String) { self.instance1::<"set_Host", System::String, ()>(a1) }
    pub fn get_port(self) -> i32 { self.instance0::<"get_Port", i32>() }
    pub fn set_port(self, a1: i32) { self.instance1::<"set_Port", i32, ()>(a1) }
    pub fn get_path(self) -> System::String { self.instance0::<"get_Path", System::String>() }
    pub fn set_path(self, a1: System::String) { self.instance1::<"set_Path", System::String, ()>(a1) }
    pub fn get_query(self) -> System::String { self.instance0::<"get_Query", System::String>() }
    pub fn set_query(self, a1: System::String) { self.instance1::<"set_Query", System::String, ()>(a1) }
    pub fn get_fragment(self) -> System::String { self.instance0::<"get_Fragment", System::String>() }
    pub fn set_fragment(self, a1: System::String) { self.instance1::<"set_Fragment", System::String, ()>(a1) }
    pub fn get_uri(self) -> System::Uri { self.instance0::<"get_Uri", System::Uri>() }
    pub fn equals(self, a1: System::Object) -> bool { self.instance1::<"Equals", System::Object, bool>(a1) }
    pub fn get_hash_code(self) -> i32 { self.virt0::<"GetHashCode", i32>() }
    pub fn to_string(self) -> System::String { self.virt0::<"ToString", System::String>() }
    pub fn new() -> Self { Self::ctor0() }
}
pub type UriFormatException =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.UriFormatException">;
use super::*;
impl From<UriFormatException> for System::FormatException {
 fn from(v:UriFormatException)->System::FormatException{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::FormatException,UriFormatException>(v)
}} 
impl UriFormatException {
    pub fn new() -> Self { Self::ctor0() }
}
pub type HttpStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.HttpStyleUriParser">;
use super::*;
impl From<HttpStyleUriParser> for System::UriParser {
 fn from(v:HttpStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,HttpStyleUriParser>(v)
}} 
impl HttpStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FtpStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.FtpStyleUriParser">;
use super::*;
impl From<FtpStyleUriParser> for System::UriParser {
 fn from(v:FtpStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,FtpStyleUriParser>(v)
}} 
impl FtpStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type FileStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.FileStyleUriParser">;
use super::*;
impl From<FileStyleUriParser> for System::UriParser {
 fn from(v:FileStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,FileStyleUriParser>(v)
}} 
impl FileStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NewsStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.NewsStyleUriParser">;
use super::*;
impl From<NewsStyleUriParser> for System::UriParser {
 fn from(v:NewsStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,NewsStyleUriParser>(v)
}} 
impl NewsStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type GopherStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.GopherStyleUriParser">;
use super::*;
impl From<GopherStyleUriParser> for System::UriParser {
 fn from(v:GopherStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,GopherStyleUriParser>(v)
}} 
impl GopherStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type LdapStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.LdapStyleUriParser">;
use super::*;
impl From<LdapStyleUriParser> for System::UriParser {
 fn from(v:LdapStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,LdapStyleUriParser>(v)
}} 
impl LdapStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NetPipeStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.NetPipeStyleUriParser">;
use super::*;
impl From<NetPipeStyleUriParser> for System::UriParser {
 fn from(v:NetPipeStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,NetPipeStyleUriParser>(v)
}} 
impl NetPipeStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type NetTcpStyleUriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.NetTcpStyleUriParser">;
use super::*;
impl From<NetTcpStyleUriParser> for System::UriParser {
 fn from(v:NetTcpStyleUriParser)->System::UriParser{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::UriParser,NetTcpStyleUriParser>(v)
}} 
impl NetTcpStyleUriParser {
    pub fn new() -> Self { Self::ctor0() }
}
pub type UriParser =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.Uri","System.UriParser">;
use super::*;
impl From<UriParser> for System::Object {
 fn from(v:UriParser)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,UriParser>(v)
}} 
impl UriParser {
    pub fn is_known_scheme(a1: System::String) -> bool { Self::static1::<"IsKnownScheme", System::String, bool>(a1) }
}
}
pub mod Internal{
pub type Console =  crate::intrinsics::RustcCLRInteropManagedClass<"System.Private.CoreLib","Internal.Console">;
use super::*;
impl From<Console> for System::Object {
 fn from(v:Console)->System::Object{
crate::intrinsics::rustc_clr_interop_managed_checked_cast::<System::Object,Console>(v)
}} 
impl Console {
    pub fn write_line(a1: System::String) { Self::static1::<"WriteLine", System::String, ()>(a1) }
    pub fn write(a1: System::String) { Self::static1::<"Write", System::String, ()>(a1) }
}
}
