# Channel pointers are keyed by release target, not by channel

LTS / latest / dogfood are channels, and each release target moves along them on its own schedule: a macOS web build can be published to latest while the same version is still waiting for App Store review on MAS. A single pointer per channel would therefore have to either lie about one of the targets or serialize releases across targets nobody wants serialized, so the console keeps one pointer per (发布目标, 通道) pair, and the release board is that matrix. Considered one global pointer per channel
(rejected: cannot express "MAS still on the previous version"), and deriving the pointer from which release assets exist (rejected: assets say a build shipped, not which channel it is promoted on).
