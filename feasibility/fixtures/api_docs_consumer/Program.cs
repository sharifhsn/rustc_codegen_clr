using System.Reflection;
using Mycorrhiza.Interop.Helpers;

static void Require(bool condition, string message)
{
    if (!condition)
        throw new InvalidOperationException(message);
}

var nullability = new NullabilityInfoContext();

Require(MainModule.greet("docs") == "Hello, docs, from Rust!", "exported method was not callable");

var greet = typeof(MainModule).GetMethod("greet", BindingFlags.Public | BindingFlags.Static)!;
static RustApiAttribute Marker(ICustomAttributeProvider target, string name) =>
    target.GetCustomAttributes(typeof(RustApiAttribute), false)
        .Cast<RustApiAttribute>()
        .Single(attribute => attribute.Name == name);
var greetMarker = Marker(greet, "greet");
Require(greetMarker.Stable && greetMarker.Order == 1, "method custom attribute arguments were lost");
Require(Marker(greet.ReturnParameter, "greeting-result") is not null, "return custom attribute was missing");
Require(Marker(greet.GetParameters()[0], "greeting-name") is not null, "parameter custom attribute was missing");
Require(greet.GetParameters() is [{ Name: "name" }], "greet parameter metadata did not preserve its Rust name");
Require(
    nullability.Create(greet.GetParameters()[0]).ReadState == NullabilityState.NotNull
        && nullability.Create(greet.ReturnParameter).ReadState == NullabilityState.NotNull,
    "required dotnet_export string metadata was missing"
);
var maybeGreet = typeof(MainModule).GetMethod("maybe_greet", BindingFlags.Public | BindingFlags.Static)!;
Require(
    nullability.Create(maybeGreet.GetParameters()[0]).ReadState == NullabilityState.Nullable
        && nullability.Create(maybeGreet.ReturnParameter).ReadState == NullabilityState.Nullable,
    "nullable dotnet_export string metadata was missing"
);
Require(maybeGreet.Invoke(null, [null]) is null, "nullable dotnet_export did not round-trip null");
try
{
    MainModule.typed_failure();
    throw new InvalidOperationException("typed Rust error did not throw");
}
catch (Mycorrhiza.Interop.Helpers.RustArgumentException error)
{
    Require(error is ArgumentException, "typed Rust argument error lost its familiar managed base");
    Require(error.Message == "native argument rejected", "typed Rust error lost its message");
    Require(error.HasNativeStatus && error.NativeStatus == 4_221, "typed Rust error lost its native status");
}

var constructor = typeof(RiskQuote).GetConstructor([typeof(int), typeof(bool)]);
Require(constructor is not null, "documented primary constructor was absent from the runtime assembly");
Require(
    constructor!.GetParameters() is [{ Name: "value" }, { Name: "active" }],
    "constructor parameter metadata did not match the XML documentation names"
);
var quote = (RiskQuote)constructor.Invoke([17, true]);
Require(quote.Value == 17 && quote.Active, "documented DTO shape was not callable");

var profileConstructor = typeof(NullableProfile).GetConstructor([typeof(string), typeof(string)]);
Require(profileConstructor is not null, "nullable DTO primary constructor was absent");
var profileConstructorParameters = profileConstructor!.GetParameters();
Require(
    nullability.Create(profileConstructorParameters[0]).ReadState == NullabilityState.NotNull
        && nullability.Create(profileConstructorParameters[1]).ReadState == NullabilityState.Nullable,
    "DTO constructor required/optional string metadata was incorrect"
);
var profile = (NullableProfile)profileConstructor.Invoke(["required", null]);
Require(
    typeof(NullableProfile)
        .GetFields(BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.Instance)
        .Any(field => field.GetCustomAttributes<RustApiAttribute>().Any(attribute => attribute.Name == "required-field")),
    "field custom attribute was missing"
);
Require(
    Marker(typeof(NullableProfile).GetProperty("RequiredName")!, "required-property").Stable,
    "property custom attribute was missing or lost its named argument"
);
Require(
    profile.RequiredName == "required" && profile.OptionalName is null,
    "DTO required/optional strings did not cross the managed constructor and properties"
);
Require(
    nullability.Create(typeof(NullableProfile).GetProperty("RequiredName")!).ReadState
        == NullabilityState.NotNull
        && nullability.Create(typeof(NullableProfile).GetProperty("OptionalName")!).ReadState
        == NullabilityState.Nullable,
    "DTO property required/optional string metadata was incorrect"
);

var calculator = typeof(MainModule).Assembly.GetType("DocumentationCalculator");
Require(calculator is { IsPublic: true }, "documented generated method owner was not a public runtime type");
Require(
    Marker(calculator!.GetField("MarkerState", BindingFlags.Public | BindingFlags.Static)!, "static-field") is not null,
    "static field custom attribute was missing"
);
var project = calculator!.GetMethod("Project", BindingFlags.Public | BindingFlags.Static)!;
Require(Marker(project, "project") is not null, "generated method custom attribute was missing");
Require(Marker(project.ReturnParameter, "project-result") is not null, "generated return custom attribute was missing");
Require(Marker(project.GetParameters()[0], "project-periods") is not null, "generated parameter custom attribute was missing");
Require(
    project.GetParameters() is [{ Name: "periods" }],
    "generated-method parameter metadata did not match the XML documentation name"
);
Require((int)project.Invoke(null, [3])! == 3, "documented generated method was not callable");

var requiredLabel = calculator.GetMethod("RequiredLabel", BindingFlags.Public | BindingFlags.Static)!;
var requiredLabelParameter = nullability.Create(requiredLabel.GetParameters()[0]);
var requiredLabelReturn = nullability.Create(requiredLabel.ReturnParameter);
Require(
    requiredLabelParameter.ReadState == NullabilityState.NotNull
        && requiredLabelReturn.ReadState == NullabilityState.NotNull,
    "required managed strings were not marked non-null"
);
string requiredValue = (string)requiredLabel.Invoke(null, ["required"])!;
Require(requiredValue == "required", "required managed string was not callable");

var optionalLabel = calculator.GetMethod("OptionalLabel", BindingFlags.Public | BindingFlags.Static)!;
var optionalLabelParameter = nullability.Create(optionalLabel.GetParameters()[0]);
var optionalLabelReturn = nullability.Create(optionalLabel.ReturnParameter);
Require(
    optionalLabelParameter.ReadState == NullabilityState.Nullable
        && optionalLabelReturn.ReadState == NullabilityState.Nullable,
    "ManagedOption<string> was not exposed as string?"
);
string? optionalValue = (string?)optionalLabel.Invoke(null, [null]);
Require(optionalValue is null, "nullable managed string did not round-trip null");

var documentedBox = typeof(IDocumentedBox<>);
Require(documentedBox.IsInterface, "documented generic interface was not emitted as an interface");
Require(
    documentedBox.GetGenericArguments() is [{ Name: "T" }],
    "interface generic-parameter metadata did not preserve T"
);
var put = documentedBox.GetMethod("Put")!;
var putParameters = put.GetParameters();
Require(
    putParameters is [{ Name: "value" }]
        && putParameters[0].ParameterType == documentedBox.GetGenericArguments()[0],
    $"interface method metadata mismatch: {put}; parameters: "
        + string.Join(", ", putParameters.Select(value => $"{value.Name}:{value.ParameterType}"))
);
var echo = documentedBox.GetMethod("Echo")!;
Require(
    echo.IsGenericMethodDefinition
        && echo.GetGenericArguments() is [{ Name: "U" }]
        && echo.GetParameters() is [{ Name: "value" }]
        && echo.GetParameters()[0].ParameterType == echo.GetGenericArguments()[0]
        && echo.ReturnType == echo.GetGenericArguments()[0],
    "generic interface-method metadata did not preserve U or the value parameter"
);
var count = documentedBox.GetProperty("Count");
Require(
    count?.PropertyType == typeof(int),
    "documented interface property was absent or had the wrong type"
);
Require(Marker(count!, "count-property") is not null, "interface property custom attribute was missing");
var interfaceRequiredLabel = documentedBox.GetMethod("RequiredLabel")!;
Require(
    nullability.Create(interfaceRequiredLabel.GetParameters()[0]).ReadState
        == NullabilityState.NotNull
        && nullability.Create(interfaceRequiredLabel.ReturnParameter).ReadState
        == NullabilityState.NotNull,
    "interface required-string nullability metadata was missing"
);
var interfaceOptionalLabel = documentedBox.GetMethod("OptionalLabel")!;
Require(
    nullability.Create(interfaceOptionalLabel.GetParameters()[0]).ReadState
        == NullabilityState.Nullable
        && nullability.Create(interfaceOptionalLabel.ReturnParameter).ReadState
        == NullabilityState.Nullable,
    "interface optional-string nullability metadata was missing"
);
Require(
    nullability.Create(documentedBox.GetProperty("OptionalName")!).ReadState
        == NullabilityState.Nullable,
    "interface nullable property metadata was missing"
);

Console.WriteLine("api docs clean consumer: PASS");
