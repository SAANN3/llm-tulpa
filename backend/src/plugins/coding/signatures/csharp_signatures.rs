use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolves a NuGet package's assembly by asking `dotnet nuget locals` for the *actual*
/// global packages folder (never a hardcoded `~/.nuget/packages`, since `NUGET_PACKAGES`
/// can move it), then reads its exported types/members via a throwaway `dotnet run`
/// program that loads the assembly with `System.Reflection` — .NET has no built-in
/// decompile-to-signatures CLI the way Java's `javap` or Go's `go doc` do, so this is
/// the equivalent built by hand out of the reflection API instead.
///
/// Best-effort, not exhaustive: needs the package already restored at least once by the
/// real project (same precondition as Node needing `npm install` done first); a
/// property's getter/setter show up as plain `get_X`/`set_X` methods rather than a real
/// `X { get; set; }` (except where explicitly handled below), and this doesn't reproduce
/// full type constraints or nullable-reference annotations, only parameter/return types.
pub fn get_dependency_signatures(_repo_path: &Path, dependency: &str) -> Result<String, String> {
    ensure_dotnet_available()?;

    let packages_root = global_packages_folder()?;
    let package_dir = highest_version_dir(&packages_root.join(dependency.to_lowercase())).ok_or_else(|| {
        format!(
            "'{dependency}' isn't under the NuGet global packages folder ('{}') — check it's \
             actually a dependency and has been restored at least once (`dotnet restore`)",
            packages_root.display()
        )
    })?;

    let assembly = find_assembly(&package_dir, dependency)
        .ok_or_else(|| format!("found '{dependency}' under {} but no usable assembly in its lib/ folder", package_dir.display()))?;

    run_reflection_dump(&assembly, dependency)
}

fn ensure_dotnet_available() -> Result<(), String> {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .map(|_| ())
        .map_err(|_| "`dotnet` isn't available — this tool needs it to resolve NuGet packages and read their exported types".to_string())
}

/// Parses `dotnet nuget locals global-packages --list`'s `global-packages: <path>` line —
/// the rest of its output (a one-time "Welcome to .NET" banner on a machine's very first
/// run, telemetry notices) is noise this doesn't otherwise rely on.
fn global_packages_folder() -> Result<PathBuf, String> {
    let output = Command::new("dotnet")
        .args(["nuget", "locals", "global-packages", "--list"])
        .output()
        .map_err(|e| format!("couldn't run `dotnet nuget locals`: {e}"))?;

    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.split_once("global-packages:").map(|(_, path)| PathBuf::from(path.trim())))
        .ok_or_else(|| "couldn't parse the NuGet global packages folder from `dotnet nuget locals`".to_string())
}

fn highest_version_dir(package_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(package_dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .max_by(|a, b| compare_versions(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()))
        .map(|e| e.path())
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect() };
    parse(a).cmp(&parse(b))
}

/// Picks the best available target-framework folder under `lib/` — newest .NET first,
/// then `netstandard`, then legacy .NET Framework monikers as a last resort — and
/// returns the first non-resource `.dll` in it. A package that ships several assemblies
/// under the same tfm (rare) only has its first one read.
fn find_assembly(package_dir: &Path, dependency: &str) -> Option<PathBuf> {
    const TFM_PREFERENCE: &[&str] = &[
        "net10.0", "net9.0", "net8.0", "net7.0", "net6.0", "net5.0", "netstandard2.1", "netstandard2.0", "netstandard1.6",
        "netstandard1.3", "netstandard1.0", "net48", "net472", "net471", "net47", "net462", "net461", "net46", "net45", "net40",
        "net35", "net20",
    ];

    let lib_dir = package_dir.join("lib");
    let _ = dependency; // kept for a clearer error message upstream; not needed for the search itself
    for tfm in TFM_PREFERENCE {
        let tfm_dir = lib_dir.join(tfm);
        if let Some(dll) = first_dll_in(&tfm_dir) {
            return Some(dll);
        }
    }
    // Fall back to whatever tfm folder exists, in case the package ships one this list
    // doesn't know about yet.
    std::fs::read_dir(&lib_dir).ok()?.filter_map(Result::ok).find_map(|e| first_dll_in(&e.path()))
}

fn first_dll_in(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "dll") && !p.to_string_lossy().ends_with(".resources.dll"))
}

/// Builds a throwaway console project referencing `assembly` directly (via `HintPath`,
/// no NuGet restore of its own needed) and runs it — `dotnet run`'s implicit framework
/// reference resolves against the locally installed SDK, no network required, the same
/// way `cargo run`/`go run` on a trivial program don't need one either.
fn run_reflection_dump(assembly: &Path, dependency: &str) -> Result<String, String> {
    let tfm = target_framework_moniker();
    let scratch = std::env::temp_dir().join(format!("code-instruments-cs-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).map_err(|e| format!("couldn't create scratch dir: {e}"))?;

    let csproj = format!(
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>{tfm}</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
  <ItemGroup>
    <Reference Include="TargetAssembly">
      <HintPath>{}</HintPath>
    </Reference>
  </ItemGroup>
</Project>
"#,
        assembly.display()
    );

    let result = (|| -> Result<String, String> {
        std::fs::write(scratch.join("dumper.csproj"), csproj).map_err(|e| e.to_string())?;
        std::fs::write(scratch.join("Program.cs"), REFLECTION_DUMP_PROGRAM).map_err(|e| e.to_string())?;

        let output = Command::new("dotnet")
            .args(["run", "--project"])
            .arg(&scratch)
            .env("CI_ASSEMBLY", assembly)
            .output()
            .map_err(|e| format!("couldn't run `dotnet run`: {e}"))?;

        if !output.status.success() {
            return Err(format!("reflection dump failed for '{dependency}': {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })();

    let _ = std::fs::remove_dir_all(&scratch);
    result
}

fn target_framework_moniker() -> String {
    Command::new("dotnet")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().split('.').next().map(str::to_string))
        .map(|major| format!("net{major}.0"))
        .unwrap_or_else(|| "net8.0".to_string())
}

/// Reads the target assembly's path from `CI_ASSEMBLY` (an environment variable, not
/// interpolated into this source), reflects over its exported types, and prints a
/// trimmed, typed view: constructors and methods with real parameter/return types,
/// properties collapsed to `Type Name { get; set; }` rather than raw `get_X`/`set_X`
/// accessor methods.
const REFLECTION_DUMP_PROGRAM: &str = r#"
using System.Reflection;

var path = Environment.GetEnvironmentVariable("CI_ASSEMBLY")!;
var asm = Assembly.LoadFrom(path);

string RenderType(Type t)
{
    if (t.IsByRef) return RenderType(t.GetElementType()!);
    if (!t.IsGenericType) return t.Name;
    var name = t.Name.Split('`')[0];
    var args = string.Join(", ", t.GetGenericArguments().Select(RenderType));
    return $"{name}<{args}>";
}

string RenderParams(MethodBase m) =>
    string.Join(", ", m.GetParameters().Select(p => $"{RenderType(p.ParameterType)} {p.Name}"));

const int MAX_TYPES = 150;
int count = 0;

foreach (var type in asm.GetExportedTypes().OrderBy(t => t.FullName))
{
    if (count++ >= MAX_TYPES)
    {
        Console.WriteLine($"[... stopped after {MAX_TYPES} types, the assembly has more ...]");
        break;
    }

    var kind = type.IsInterface ? "interface" : type.IsEnum ? "enum" : type.IsValueType ? "struct" : "class";
    Console.WriteLine($"{kind} {RenderType(type)}");

    if (type.IsEnum)
    {
        foreach (var name in Enum.GetNames(type)) Console.WriteLine($"    {name}");
        Console.WriteLine();
        continue;
    }

    foreach (var ctor in type.GetConstructors())
        Console.WriteLine($"    {type.Name}({RenderParams(ctor)})");

    foreach (var prop in type.GetProperties(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly))
    {
        var accessors = (prop.CanRead ? "get; " : "") + (prop.CanWrite ? "set; " : "");
        Console.WriteLine($"    {RenderType(prop.PropertyType)} {prop.Name} {{ {accessors}}}");
    }

    var accessorNames = new HashSet<string>(
        type.GetProperties(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly)
            .SelectMany(p => new[] { p.GetMethod?.Name, p.SetMethod?.Name })
            .Where(n => n != null)!
    );
    foreach (var method in type.GetMethods(BindingFlags.Public | BindingFlags.Instance | BindingFlags.Static | BindingFlags.DeclaredOnly))
    {
        if (accessorNames.Contains(method.Name) || method.Name.StartsWith("add_") || method.Name.StartsWith("remove_")) continue;
        Console.WriteLine($"    {RenderType(method.ReturnType)} {method.Name}({RenderParams(method)})");
    }

    Console.WriteLine();
}
"#;
