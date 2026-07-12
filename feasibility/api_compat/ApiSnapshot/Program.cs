using System.Collections.Immutable;
using System.Reflection;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: ApiSnapshot <assembly.dll>");
    return 2;
}

using var stream = File.OpenRead(args[0]);
using var pe = new PEReader(stream);
if (!pe.HasMetadata)
    throw new InvalidDataException($"{args[0]} has no CLR metadata");

var reader = pe.GetMetadataReader();
var formatter = new ApiFormatter(reader);
foreach (var line in formatter.Snapshot().Order(StringComparer.Ordinal))
    Console.WriteLine(line);
return 0;

sealed class ApiFormatter(MetadataReader reader)
{
    private static readonly HashSet<string> NullabilityAttributes = new(StringComparer.Ordinal)
    {
        "System.Runtime.CompilerServices.NullableAttribute",
        "System.Runtime.CompilerServices.NullableContextAttribute",
        "System.Runtime.CompilerServices.RequiredMemberAttribute",
        "System.Diagnostics.CodeAnalysis.AllowNullAttribute",
        "System.Diagnostics.CodeAnalysis.DisallowNullAttribute",
        "System.Diagnostics.CodeAnalysis.MaybeNullAttribute",
        "System.Diagnostics.CodeAnalysis.NotNullAttribute",
        "System.Diagnostics.CodeAnalysis.NotNullIfNotNullAttribute",
        "System.Diagnostics.CodeAnalysis.MemberNotNullAttribute",
        "System.Diagnostics.CodeAnalysis.MemberNotNullWhenAttribute",
    };

    private readonly TypeNames names = new(reader);

    public IEnumerable<string> Snapshot()
    {
        foreach (var handle in reader.TypeDefinitions)
        {
            var type = reader.GetTypeDefinition(handle);
            if (!IsPublic(type.Attributes)) continue;
            var typeName = names.Definition(handle);
            var kind = IsEnum(type) ? "enum" : type.Attributes.HasFlag(TypeAttributes.Interface) ? "interface" : "type";
            yield return $"{kind} {typeName}{Attributes(type.GetCustomAttributes())}";

            if (kind == "enum")
            {
                foreach (var fieldHandle in type.GetFields())
                {
                    var field = reader.GetFieldDefinition(fieldHandle);
                    if (!field.Attributes.HasFlag(FieldAttributes.Literal) || !field.Attributes.HasFlag(FieldAttributes.Public)) continue;
                    yield return $"enum-member {typeName}.{reader.GetString(field.Name)}={Constant(field.GetDefaultValue())}";
                }
            }

            var propertyAccessors = new HashSet<MethodDefinitionHandle>();
            foreach (var propertyHandle in type.GetProperties())
            {
                var property = reader.GetPropertyDefinition(propertyHandle);
                var accessors = property.GetAccessors();
                if (!accessors.Getter.IsNil) propertyAccessors.Add(accessors.Getter);
                if (!accessors.Setter.IsNil) propertyAccessors.Add(accessors.Setter);
                if (!IsPublic(accessors.Getter) && !IsPublic(accessors.Setter)) continue;
                var signature = property.DecodeSignature(names, null);
                var index = signature.ParameterTypes.Length == 0 ? "" : $"[{string.Join(",", signature.ParameterTypes)}]";
                var access = $"{{{(IsPublic(accessors.Getter) ? "get;" : "")}{(IsPublic(accessors.Setter) ? "set;" : "")}}}";
                yield return $"property {typeName}.{reader.GetString(property.Name)}{index}:{signature.ReturnType} {access}{Attributes(property.GetCustomAttributes())}";
            }

            foreach (var methodHandle in type.GetMethods())
            {
                var method = reader.GetMethodDefinition(methodHandle);
                if (!method.Attributes.HasFlag(MethodAttributes.Public) || propertyAccessors.Contains(methodHandle)) continue;
                var methodName = reader.GetString(method.Name);
                if (methodName == ".cctor") continue;
                var signature = method.DecodeSignature(names, null);
                var parameters = Parameters(method, signature.ParameterTypes);
                var attrs = Attributes(method.GetCustomAttributes());
                if (methodName == ".ctor")
                    yield return $"constructor {typeName}({parameters}){attrs}";
                else
                    yield return $"method {typeName}.{methodName}({parameters}):{signature.ReturnType}{attrs}";
            }
        }
    }

    private string Parameters(MethodDefinition method, ImmutableArray<string> types)
    {
        var attributes = method.GetParameters().Select(reader.GetParameter)
            .Where(p => p.SequenceNumber > 0).ToDictionary(p => (int)p.SequenceNumber);
        return string.Join(",", types.Select((type, index) =>
            attributes.TryGetValue(index + 1, out var parameter)
                ? type + Attributes(parameter.GetCustomAttributes())
                : type));
    }

    private bool IsPublic(MethodDefinitionHandle handle) =>
        !handle.IsNil && reader.GetMethodDefinition(handle).Attributes.HasFlag(MethodAttributes.Public);

    private static bool IsPublic(TypeAttributes attributes) =>
        attributes.HasFlag(TypeAttributes.Public) || attributes.HasFlag(TypeAttributes.NestedPublic);

    private bool IsEnum(TypeDefinition type) => names.Entity(type.BaseType) == "System.Enum";

    private string Attributes(CustomAttributeHandleCollection handles)
    {
        var values = new List<string>();
        foreach (var handle in handles)
        {
            var attribute = reader.GetCustomAttribute(handle);
            var name = AttributeType(attribute.Constructor);
            if (!NullabilityAttributes.Contains(name)) continue;
            values.Add($"{name}({Convert.ToHexString(reader.GetBlobBytes(attribute.Value))})");
        }
        values.Sort(StringComparer.Ordinal);
        return values.Count == 0 ? "" : " attrs=[" + string.Join(",", values) + "]";
    }

    private string AttributeType(EntityHandle constructor) => constructor.Kind switch
    {
        HandleKind.MemberReference => names.Entity(reader.GetMemberReference((MemberReferenceHandle)constructor).Parent),
        HandleKind.MethodDefinition => names.Definition(reader.GetMethodDefinition((MethodDefinitionHandle)constructor).GetDeclaringType()),
        _ => throw new BadImageFormatException($"unsupported attribute constructor {constructor.Kind}"),
    };

    private string Constant(ConstantHandle handle)
    {
        if (handle.IsNil) return "null";
        var constant = reader.GetConstant(handle);
        var blob = reader.GetBlobReader(constant.Value);
        object? value = constant.TypeCode switch
        {
            ConstantTypeCode.Boolean => blob.ReadBoolean(),
            ConstantTypeCode.Char => (char)blob.ReadUInt16(),
            ConstantTypeCode.SByte => blob.ReadSByte(),
            ConstantTypeCode.Byte => blob.ReadByte(),
            ConstantTypeCode.Int16 => blob.ReadInt16(),
            ConstantTypeCode.UInt16 => blob.ReadUInt16(),
            ConstantTypeCode.Int32 => blob.ReadInt32(),
            ConstantTypeCode.UInt32 => blob.ReadUInt32(),
            ConstantTypeCode.Int64 => blob.ReadInt64(),
            ConstantTypeCode.UInt64 => blob.ReadUInt64(),
            ConstantTypeCode.Single => blob.ReadSingle(),
            ConstantTypeCode.Double => blob.ReadDouble(),
            ConstantTypeCode.String => blob.ReadUTF16(blob.Length),
            ConstantTypeCode.NullReference => null,
            _ => Convert.ToHexString(reader.GetBlobBytes(constant.Value)),
        };
        return value switch { null => "null", bool b => b ? "true" : "false", string s => $"\"{s}\"", _ => Convert.ToString(value, System.Globalization.CultureInfo.InvariantCulture)! };
    }
}

sealed class TypeNames(MetadataReader reader) : ISignatureTypeProvider<string, object?>
{
    public string Definition(TypeDefinitionHandle handle)
    {
        var type = reader.GetTypeDefinition(handle);
        var name = reader.GetString(type.Name);
        var declaring = type.GetDeclaringType();
        if (!declaring.IsNil) return Definition(declaring) + "+" + name;
        var ns = reader.GetString(type.Namespace);
        return string.IsNullOrEmpty(ns) ? name : ns + "." + name;
    }

    public string Entity(EntityHandle handle) => handle.Kind switch
    {
        HandleKind.TypeDefinition => Definition((TypeDefinitionHandle)handle),
        HandleKind.TypeReference => Reference((TypeReferenceHandle)handle),
        HandleKind.TypeSpecification => reader.GetTypeSpecification((TypeSpecificationHandle)handle).DecodeSignature(this, null),
        _ => "<" + handle.Kind + ">",
    };

    private string Reference(TypeReferenceHandle handle)
    {
        var type = reader.GetTypeReference(handle);
        var name = reader.GetString(type.Name);
        return type.ResolutionScope.Kind == HandleKind.TypeReference
            ? Reference((TypeReferenceHandle)type.ResolutionScope) + "+" + name
            : string.IsNullOrEmpty(reader.GetString(type.Namespace)) ? name : reader.GetString(type.Namespace) + "." + name;
    }

    public string GetArrayType(string elementType, ArrayShape shape) => elementType + "[" + new string(',', shape.Rank - 1) + "]";
    public string GetByReferenceType(string elementType) => elementType + "&";
    public string GetFunctionPointerType(MethodSignature<string> signature) => "fnptr(" + string.Join(",", signature.ParameterTypes) + ")->" + signature.ReturnType;
    public string GetGenericInstantiation(string genericType, ImmutableArray<string> typeArguments) => genericType + "<" + string.Join(",", typeArguments) + ">";
    public string GetGenericMethodParameter(object? genericContext, int index) => "!!" + index;
    public string GetGenericTypeParameter(object? genericContext, int index) => "!" + index;
    public string GetModifiedType(string modifierType, string unmodifiedType, bool isRequired) => (isRequired ? "modreq(" : "modopt(") + modifierType + ")" + unmodifiedType;
    public string GetPinnedType(string elementType) => elementType + " pinned";
    public string GetPointerType(string elementType) => elementType + "*";
    public string GetPrimitiveType(PrimitiveTypeCode typeCode) => "System." + typeCode;
    public string GetSZArrayType(string elementType) => elementType + "[]";
    public string GetTypeFromDefinition(MetadataReader metadataReader, TypeDefinitionHandle handle, byte rawTypeKind) => Definition(handle);
    public string GetTypeFromReference(MetadataReader metadataReader, TypeReferenceHandle handle, byte rawTypeKind) => Reference(handle);
    public string GetTypeFromSpecification(MetadataReader metadataReader, object? genericContext, TypeSpecificationHandle handle, byte rawTypeKind) => metadataReader.GetTypeSpecification(handle).DecodeSignature(this, genericContext);
}
