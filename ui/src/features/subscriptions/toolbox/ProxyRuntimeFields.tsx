import { Alert, Form, type FormInstance, Input, InputNumber, Password, Select, Switch, TextArea } from "@acme/components";
import { useTranslation } from "react-i18next";
import type { LinuxNetworkInventory } from "@/lib/types";

interface Props {
  visible: boolean;
  supportsLocalProxy: boolean;
  supportsTransparent: boolean;
  supportsManagement: boolean;
  features: Set<string>;
  form: FormInstance;
  transparentMode?: string;
  tunInterfaceMode?: string;
  networkInventory?: LinuxNetworkInventory;
}

export function ProxyRuntimeFields({
  visible,
  supportsLocalProxy,
  supportsTransparent,
  supportsManagement,
  features,
  form,
  transparentMode,
  tunInterfaceMode,
  networkInventory,
}: Props) {
  const { t } = useTranslation();
  return 			<div className={visible ? "space-y-5" : "hidden"}>
				{supportsLocalProxy ? <section className="space-y-4">
					<div className="grid gap-4 md:grid-cols-2">
						<Form.Item label={t("proxy.form.localProxySOCKSPort")} name="localProxySOCKSPort">
							<InputNumber min={1} max={65535} className="w-full" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyHTTPPort")} name="localProxyHTTPPort">
							<InputNumber min={1} max={65535} className="w-full" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyUsername")} name="localProxyUsername">
							<Input autoComplete="username" />
						</Form.Item>
						<Form.Item label={t("proxy.form.localProxyPassword")} name="localProxyPassword">
							<Password autoComplete="new-password" />
						</Form.Item>
					</div>
				</section> : null}
				{supportsTransparent ? <section className="space-y-4">
					<Form.Item label={t("proxy.form.transparentMode")} name="transparentMode">
						<Select
							options={[
								...(features.has("transparent.tun") ? [{ value: "tun-router", label: t("proxy.form.transparentModeTun") }] : []),
								...(features.has("transparent.tproxy") ? [{ value: "tproxy", label: t("proxy.form.transparentModeTProxy") }] : []),
								...(features.has("transparent.ebpf") ? [{ value: "ebpf-router", label: t("proxy.form.transparentModeEBPF") }] : []),
								{ value: "disabled", label: t("proxy.form.transparentModeDisabled") },
							]}
							onChange={(value) => {
								if ((value === "tproxy" || value === "ebpf-router") && (form.getFieldValue("tproxyLANInterfaces") as string[] | undefined)?.length === 0 && networkInventory?.recommended_lan_interfaces.length) {
									form.setFieldValue("tproxyLANInterfaces", networkInventory.recommended_lan_interfaces);
								}
								if (value === "ebpf-router" && !form.getFieldValue("ebpfWANInterface")) {
									form.setFieldValue("ebpfWANInterface", networkInventory?.default_interface || "auto");
								}
							}}
						/>
					</Form.Item>
					{transparentMode === "tun-router" && features.has("transparent.tun") ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item
									label={t("proxy.form.tunInterface")}
									name="tunInterfaceName"
									rules={[{ validator: (_rule, value) => {
										if (!String(value ?? "").trim()) {
											throw new Error(t("proxy.form.tunInterfaceRequired"));
										}
									} }]}
								>
									<Input />
								</Form.Item>
								{features.has("transparent.tun.address") ? <Form.Item label={t("proxy.form.tunAddress")} name="tunAddress">
									<Input placeholder={t("proxy.form.tunAddressAuto")} />
								</Form.Item> : null}
							</div>
							{features.has("transparent.interface_policy") ? (
								<div className="grid gap-4 md:grid-cols-2">
									<Form.Item label={t("proxy.form.tunInterfacePolicy")} name="tunInterfaceMode">
										<Select options={[
											{ value: "all", label: t("proxy.form.tunInterfaceAll") },
											{ value: "include", label: t("proxy.form.tunInterfaceInclude") },
											{ value: "exclude", label: t("proxy.form.tunInterfaceExclude") },
										]} />
									</Form.Item>
									{tunInterfaceMode !== "all" ? <Form.Item label={t("proxy.form.tunInterfaces")} name="tunInterfaces">
										<Select mode="tags" options={(networkInventory?.interfaces ?? []).map((item) => ({ value: item.name, label: item.name }))} />
									</Form.Item> : null}
								</div>
							) : null}
							<Form.Item label={t("proxy.form.tunRouteExclusions")} name="tunRouteExclusions">
								<TextArea rows={3} />
							</Form.Item>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tunAutoExcludeLocal")} name="tunAutoExcludeLocal" valuePropName="checked">
									<Switch />
								</Form.Item>
								<Form.Item label={t("proxy.form.tunAutoExcludeVPN")} name="tunAutoExcludeVPN" valuePropName="checked">
									<Switch />
								</Form.Item>
							</div>
						</>
					) : null}
					{transparentMode === "tproxy" && features.has("transparent.tproxy") ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.tproxyPort")} name="tproxyPort">
									<InputNumber min={1} max={65535} className="w-full" />
								</Form.Item>
								<Form.Item label={t("proxy.form.tproxyDNSPort")} name="tproxyDNSPort">
									<InputNumber min={1} max={65535} className="w-full" />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.tproxyLANInterfaces")} name="tproxyLANInterfaces">
								<Select
									mode="tags"
									showSearch
									options={(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({
										value: item.name,
										label: `${item.name} · ${item.kind}${item.default_route ? ` · ${t("proxy.form.defaultRoute")}` : ""}`,
										tagLabel: item.name,
									}))}
								/>
							</Form.Item>
							<Form.Item label={t("proxy.form.tproxyCaptureHost")} name="tproxyCaptureHost" valuePropName="checked">
								<Switch />
							</Form.Item>
						</>
					) : null}
					{transparentMode === "ebpf-router" && features.has("transparent.ebpf") ? (
						<>
							<div className="grid gap-4 md:grid-cols-2">
								<Form.Item label={t("proxy.form.ebpfWANInterface")} name="ebpfWANInterface">
									<Select options={[
										{ value: "auto", label: t("proxy.form.ebpfWANAuto") },
										...(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({ value: item.name, label: item.name })),
									]} />
								</Form.Item>
								<Form.Item label={t("proxy.form.ebpfLANInterfaces")} name="tproxyLANInterfaces">
									<Select mode="tags" showSearch options={(networkInventory?.interfaces ?? []).filter((item) => item.up).map((item) => ({ value: item.name, label: item.name }))} />
								</Form.Item>
							</div>
							<Form.Item label={t("proxy.form.ebpfAutoConfigKernel")} name="ebpfAutoConfigKernel" valuePropName="checked">
								<Switch />
							</Form.Item>
						</>
					) : null}
				</section> : null}

				{supportsManagement ? <section className="space-y-4 border-t border-[var(--border)] pt-5">
					<Alert type="warning" showIcon message={t("proxy.form.managementAPISecurityWarning")} />
					<div className="grid gap-4 md:grid-cols-2">
						<Form.Item label={t("proxy.form.managementAPIController")} name="managementAPIController">
							<Input />
						</Form.Item>
						<Form.Item label={t("proxy.form.managementAPISecret")} name="managementAPISecret">
							<Password autoComplete="new-password" />
						</Form.Item>
					</div>
					<Form.Item label={t("proxy.form.managementAPIUI")} name="managementAPIUI">
						<Input />
					</Form.Item>
					<Form.Item label={t("proxy.form.managementAPIOrigins")} name="managementAPIOrigins">
						<Select mode="tags" />
					</Form.Item>
					<Form.Item label={t("proxy.form.managementAPIPrivateNetwork")} name="managementAPIPrivateNetwork" valuePropName="checked">
						<Switch />
					</Form.Item>
				</section> : null}
			</div>;
}
