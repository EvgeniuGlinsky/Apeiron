# R8 rules for the release build.
#
# Flutter picks up this file automatically if it exists, and enables minification
# in release itself. Hence the need: the class io.apeiron.apeiron.Vault is
# called from Rust over JNI by name, and Rust will not find a class renamed by R8
# — silently, already on the device.
#
# The Flutter default (proguard-android-optimize.txt) contains
# -keepclasseswithmembernames class * { native <methods>; }, and that alone would
# most likely be enough: Vault has a native method. But relying on someone else's
# default in a place where a failure is discovered only on the phone is a bad
# trade. The rule below is explicit and will survive a change of the default.

-keep class io.apeiron.apeiron.Vault {
    *;
}

# Classes with native methods — in case there is ever more than one.
-keepclasseswithmembernames class * {
    native <methods>;
}
