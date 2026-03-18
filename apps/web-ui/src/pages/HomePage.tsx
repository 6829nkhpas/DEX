import React from "react";
import { Link } from "wouter";
import { ArrowRight, Zap, Shield, GitMerge, Cpu, Server, Activity, Lock } from "lucide-react";

export const HomePage: React.FC = () => {
    return (
        <div className="w-full flex flex-col items-center">
            {/* Background Effects */}
            <div className="fixed inset-0 pointer-events-none -z-10 overflow-hidden">
                <div className="absolute top-[-10%] left-[-10%] w-[40%] h-[40%] bg-indigo-600/10 rounded-full blur-[120px]" />
                <div className="absolute top-[40%] right-[-10%] w-[30%] h-[50%] bg-purple-600/10 rounded-full blur-[140px]" />
                <div className="absolute bottom-[-10%] left-[20%] w-[50%] h-[30%] bg-emerald-600/5 rounded-full blur-[100px]" />
            </div>

            {/* Hero Section */}
            <section className="w-full max-w-7xl px-6 py-24 md:py-32 flex flex-col items-center text-center mt-10">
                <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-indigo-500/10 border border-indigo-500/20 text-indigo-400 text-sm font-semibold mb-8 animate-fade-in">
                    <span className="relative flex h-2 w-2">
                        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-indigo-400 opacity-75"></span>
                        <span className="relative inline-flex rounded-full h-2 w-2 bg-indigo-500"></span>
                    </span>
                    Core Engine v0.1.0 Live
                </div>

                <h1 className="text-5xl md:text-7xl font-display font-extrabold tracking-tight text-white mb-6 animate-slide-up leading-tight">
                    The Next-Gen <br />
                    <span className="text-gradient-primary">Decentralized Exchange</span>
                </h1>

                <p className="text-lg md:text-xl text-slate-400 max-w-2xl mb-10 leading-relaxed animate-fade-in" style={{ animationDelay: "100ms" }}>
                    A distributed, 9-microservice exchange architecture built for high-throughput, low-latency trading.
                    Targeting <strong>p99 latency &lt; 500µs</strong> and <strong>100,000 orders/sec</strong> per symbol.
                </p>

                <div className="flex flex-col sm:flex-row gap-4 animate-fade-in" style={{ animationDelay: "200ms" }}>
                    <Link href="/trade" className="relative px-8 py-4 bg-indigo-600 hover:bg-indigo-500 text-white font-bold rounded-xl transition-all hover:scale-105 hover:shadow-[0_0_30px_rgba(99,102,241,0.4)] flex items-center justify-center gap-2 group overflow-hidden">
                        <div className="absolute inset-0 bg-gradient-to-r from-indigo-500 to-purple-500 pointer-events-none" />
                        <span className="relative z-10 flex items-center gap-2">
                            Launch UI <ArrowRight className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
                        </span>
                    </Link>
                    <a href="https://github.com" target="_blank" rel="noreferrer" className="px-8 py-4 glass-panel hover:bg-slate-800/80 text-white font-bold rounded-xl transition-all hover:scale-105 flex items-center justify-center gap-2 group">
                        <GitMerge className="w-4 h-4 text-slate-400 group-hover:text-white transition-colors" />
                        Read the Specs
                    </a>
                </div>
            </section>

            {/* Performance Metrics Section */}
            <section className="w-full max-w-7xl px-6 pb-20 grid grid-cols-2 md:grid-cols-4 gap-6 animate-fade-in" style={{ animationDelay: "300ms" }}>
                <MetricCard value="< 500µs" label="p99 Latency (HOT PATH)" />
                <MetricCard value="100k TPS" label="Per-Symbol Throughput" />
                <MetricCard value="9 Services" label="Microservice Topology" />
                <MetricCard value="T+0" label="Atomic Settlement" />
            </section>

            {/* Features Grid */}
            <section className="w-full max-w-7xl px-6 py-20">
                <div className="text-center mb-16">
                    <h2 className="text-3xl md:text-4xl font-display font-bold text-white mb-4">Engineering Excellence</h2>
                    <p className="text-slate-400 max-w-2xl mx-auto">Designed from the ground up to solve the most complex problems in modern decentralized finance.</p>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
                    <FeatureCard
                        icon={<Zap className="w-6 h-6 text-yellow-400" />}
                        title="In-Memory Matching"
                        desc="Core Rust deterministic matching engine scaled vertically per symbol. Designed for peak LMAX disruptor-pattern efficiency."
                    />
                    <FeatureCard
                        icon={<Activity className="w-6 h-6 text-emerald-400" />}
                        title="Event-Sourced State"
                        desc="Asynchronous event log for high-volume state transitions with at-least-once delivery semantics. Ensures perfect replayability."
                    />
                    <FeatureCard
                        icon={<Server className="w-6 h-6 text-indigo-400" />}
                        title="gRPC & WebSockets"
                        desc="Synchronous routing for critical-path orders, combined with optimized WebSocket adaptive batching for zero UI drift."
                    />
                    <FeatureCard
                        icon={<Shield className="w-6 h-6 text-rose-400" />}
                        title="Real-Time Risk & ADL"
                        desc="Continuous margining triggering real-time liquidation services and insurance fund auto-deleveraging under extreme conditions."
                    />
                </div>
            </section>

            {/* Tech Stack */}
            <section className="w-full max-w-6xl px-6 py-24 mb-20 relative">
                <div className="absolute inset-x-0 top-1/2 -translate-y-1/2 h-px bg-gradient-to-r from-transparent via-slate-700/50 to-transparent -z-10" />

                <h3 className="text-center text-sm font-semibold tracking-widest text-slate-500 uppercase mb-10">
                    POWERED BY MODERN TECHNOLOGY
                </h3>

                <div className="flex flex-wrap justify-center gap-4 md:gap-8">
                    <StackBadge name="Rust" highlight="border-orange-500/30 text-orange-400" />
                    <StackBadge name="React 19" highlight="border-cyan-500/30 text-cyan-400" />
                    <StackBadge name="Vite" highlight="border-purple-500/30 text-purple-400" />
                    <StackBadge name="Tailwind CSS v4" highlight="border-sky-500/30 text-sky-400" />
                    <StackBadge name="WebSockets" highlight="border-lime-500/30 text-lime-400" />
                    <StackBadge name="Decimal.js" highlight="border-slate-500/30 text-slate-300" />
                </div>
            </section>

            {/* Architecture Visualization Flow */}
            <section className="w-full max-w-5xl px-6 pb-32">
                <div className="glass-panel-heavy p-8 md:p-12 rounded-3xl border-t border-indigo-500/30 shadow-2xl relative overflow-hidden">
                    <div className="absolute -top-32 -left-32 w-64 h-64 bg-indigo-500/20 rounded-full blur-[100px]" />

                    <h2 className="text-2xl md:text-3xl font-display font-bold text-white mb-10 text-center relative z-10">How It Works</h2>

                    <div className="flex flex-col md:flex-row items-center justify-between gap-4 md:gap-0 relative z-10">
                        <ArchNode icon={<Lock className="w-6 h-6" />} title="Web Client" subtitle="React UI" color="text-indigo-400" />
                        <ArchConnector />
                        <ArchNode icon={<Server className="w-6 h-6" />} title="API Gateway" subtitle="Rate Limits & Auth" color="text-sky-400" />
                        <ArchConnector />
                        <ArchNode icon={<Cpu className="w-6 h-6" />} title="Matching Engine" subtitle="In-Memory Rust" color="text-emerald-400" />
                        <ArchConnector />
                        <ArchNode icon={<Shield className="w-6 h-6" />} title="Settlement" subtitle="Smart Contracts" color="text-purple-400" />
                    </div>
                </div>
            </section>
        </div>
    );
};

const MetricCard = ({ value, label }: { value: string, label: string }) => (
    <div className="glass-panel p-6 rounded-2xl flex flex-col items-center justify-center text-center border border-indigo-500/10 hover:border-indigo-500/30 transition-colors">
        <div className="text-3xl md:text-4xl font-display font-extrabold text-white mb-2 tracking-tight text-gradient-primary">
            {value}
        </div>
        <div className="text-sm font-semibold text-slate-400 uppercase tracking-widest">
            {label}
        </div>
    </div>
);

const FeatureCard = ({ icon, title, desc }: { icon: React.ReactNode, title: string, desc: string }) => (
    <div className="glass-panel p-6 rounded-2xl flex flex-col gap-4 group hover:-translate-y-1 transition-transform duration-300">
        <div className="w-12 h-12 rounded-xl bg-slate-800/50 border border-slate-700/50 flex items-center justify-center group-hover:scale-110 transition-transform duration-300">
            {icon}
        </div>
        <h3 className="text-lg font-bold text-white">{title}</h3>
        <p className="text-sm text-slate-400 leading-relaxed">{desc}</p>
    </div>
);

const StackBadge = ({ name, highlight }: { name: string, highlight: string }) => (
    <div className={`px-4 py-2 rounded-full glass-panel border font-mono text-sm shadow-lg ${highlight}`}>
        {name}
    </div>
);

const ArchNode = ({ icon, title, subtitle, color }: { icon: React.ReactNode, title: string, subtitle: string, color: string }) => (
    <div className="flex flex-col items-center gap-3 w-32">
        <div className={`w-16 h-16 rounded-2xl glass-panel border border-slate-700/50 flex items-center justify-center ${color} shadow-lg relative`}>
            {icon}
            <div className={`absolute inset-0 rounded-2xl border border-current opacity-20`} />
        </div>
        <div className="text-center">
            <div className="font-bold text-slate-200 text-sm whitespace-nowrap">{title}</div>
            <div className="text-xs text-slate-500 whitespace-nowrap">{subtitle}</div>
        </div>
    </div>
);

const ArchConnector = () => (
    <div className="flex-1 flex items-center justify-center w-8 md:w-auto h-8 md:h-auto">
        <div className="hidden md:block h-[2px] w-full bg-gradient-to-r from-slate-700 to-indigo-500/50 max-w-[60px]" />
        <div className="block md:hidden w-[2px] h-full bg-gradient-to-b from-slate-700 to-indigo-500/50" />
        <ArrowRight className="hidden md:block w-4 h-4 text-indigo-400 -ml-1" />
    </div>
);
