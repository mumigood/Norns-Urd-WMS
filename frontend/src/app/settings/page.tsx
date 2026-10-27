"use client";

/**
 * 系統設定 — the knobs that apply to the whole plant.
 *
 * Distinct from 基本資料, which is the data the work is *about* (items,
 * suppliers, machines). These are decisions about how the system behaves: which
 * model reads the labels, where images come from, when an alert fires. Mixing
 * the two makes it unclear which changes affect a record and which affect the
 * software.
 */

import { Alert, Button, Card, Col, Form, Input, InputNumber, Popconfirm, Radio, Row, Space, Tabs, Typography, message } from "antd";
import { useCallback, useEffect, useState } from "react";
import CameraSettingsPanel from "@/components/CameraSettingsPanel";
import UsersPanel from "@/components/UsersPanel";
import { useAuth } from "@/components/AuthGate";
import { api } from "@/lib/api";

const { Title, Text } = Typography;

export default function SettingsPage() {
  const { can } = useAuth();
  const readOnly = !can("dictionary.manage");

  return (
    <>
      <Title level={3} style={{ marginTop: 0 }}>系統設定</Title>
      <Text type="secondary" style={{ display: "block", marginBottom: 16 }}>
        整廠共用的設定。品項、廠商、機台那些「資料」在「基本資料」，這裡是「系統怎麼運作」。
      </Text>

      <Card>
        <Tabs
          items={[
            { key: "vision", label: "影像設定", children: <VisionSettings readOnly={readOnly} /> },
            { key: "alerts", label: "提醒門檻", children: <AlertSettings readOnly={readOnly} /> },
            // 只有 admin 看得到；不是靠藏選單把關，伺服器端每個端點各自檢查
            ...(can("user.manage")
              ? [{ key: "users", label: "人員與權限", children: <UsersPanel /> }]
              : []),
          ]}
        />
      </Card>
    </>
  );
}

function VisionSettings({ readOnly }: { readOnly: boolean }) {
  const [model, setModel] = useState<string>();
  const [models, setModels] = useState<{ value: string; label: string; note: string }[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const s = await api.recognitionSettings();
      setModel(s.model);
      setModels(s.models);
    } catch (e) {
      message.error((e as Error).message);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function save(next: string) {
    setBusy(true);
    try {
      await api.saveRecognition(next);
      setModel(next);
      message.success("已更改辨識模型，下一張照片就會用新的");
    } catch (e) {
      message.error((e as Error).message);
      load();
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <ApiKeySettings readOnly={readOnly} />
      <Title level={5} style={{ marginTop: 0 }}>辨識模型</Title>
      <Text type="secondary" style={{ display: "block", marginBottom: 12 }}>
        讀標籤型號與驗收章的模型。下面的秒數是拿現場實照量出來的中位值，不是估計。
      </Text>
      <Radio.Group
        value={model}
        disabled={readOnly || busy}
        onChange={(e) => save(e.target.value)}
        style={{ display: "block", marginBottom: 32 }}
      >
        <Space orientation="vertical" size={8}>
          {models.map((m) => (
            <Radio key={m.value} value={m.value}>
              <Space>
                <span>{m.label}</span>
                <Text type="secondary" style={{ fontSize: 12 }}>{m.note}</Text>
              </Space>
            </Radio>
          ))}
        </Space>
      </Radio.Group>

      <Title level={5}>影像來源</Title>
      <CameraSettingsPanel />
    </>
  );
}

function AlertSettings({ readOnly }: { readOnly: boolean }) {
  const [form] = Form.useForm();
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      form.setFieldsValue(await api.alertThresholds());
    } catch (e) {
      message.error((e as Error).message);
    }
  }, [form]);

  useEffect(() => { load(); }, [load]);

  async function save() {
    const values = await form.validateFields().catch(() => null);
    if (!values) return;
    setBusy(true);
    try {
      await api.saveAlertThresholds(values);
      message.success("已更新提醒門檻");
    } catch (e) {
      message.error((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <Alert
        type="info"
        title="這些數字決定「提醒」頁上什麼時候跳出來"
        description="調寬會漏掉真正該處理的；調窄會天天跳、然後被當背景雜訊忽略 —— 兩種失效方式都存在。"
        style={{ marginBottom: 24 }}
      />
      <Form form={form} layout="vertical" disabled={readOnly} style={{ maxWidth: 640 }}>
        <Row gutter={16}>
          <Col xs={24} md={8}>
            <Form.Item name="expiry_days" label="效期將屆（天）" extra="剩多少天以內開始提醒">
              <InputNumber min={1} max={3650} style={{ width: "100%" }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item name="stale_days" label="呆滯批次（天）" extra="進貨後放多久還沒領完">
              <InputNumber min={1} max={3650} style={{ width: "100%" }} />
            </Form.Item>
          </Col>
          <Col xs={24} md={8}>
            <Form.Item name="pending_hours" label="明細待補（小時）" extra="領走後多久還沒補產品名稱">
              <InputNumber min={1} max={720} style={{ width: "100%" }} />
            </Form.Item>
          </Col>
        </Row>
        <Button type="primary" onClick={save} loading={busy} disabled={readOnly}>儲存</Button>
      </Form>
    </>
  );
}

function ApiKeySettings({ readOnly }: { readOnly: boolean }) {
  const [status, setStatus] = useState<{ configured: boolean; from_env: boolean }>();
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const load = useCallback(async () => {
    try { setStatus(await api.apiKey()); } catch (e) { message.error((e as Error).message); }
  }, []);
  useEffect(() => { load(); }, [load]);
  async function save(value: string) {
    setBusy(true);
    try {
      await api.saveApiKey(value); setKey(""); await load();
      message.success(value ? "已儲存金鑰" : "已清除金鑰");
    } catch (e) { message.error((e as Error).message); }
    finally { setBusy(false); }
  }
  async function test() {
    setBusy(true);
    try {
      const result = await api.testApiKey(key.trim() || undefined);
      if (result.ok) message.success("辨識服務連線成功");
      else message.error(result.error || "辨識服務連線失敗");
    } catch (e) { message.error((e as Error).message); }
    finally { setBusy(false); }
  }
  return <section style={{ marginBottom: 32 }}>
    <Title level={5} style={{ marginTop: 0 }}>辨識服務金鑰</Title>
    <Text type="secondary" style={{ display: "block", marginBottom: 12 }}>
      {status ? status.from_env ? "使用伺服器環境設定的金鑰" : status.configured ? "已設定金鑰；不會顯示原值" : "尚未設定 Gemini API 金鑰" : "載入設定…"}
    </Text>
    {!status?.from_env && <Input.Password aria-label="Gemini API 金鑰" placeholder="貼上新的 Gemini API 金鑰"
      value={key} onChange={e => setKey(e.target.value)} disabled={readOnly || busy || !status}
      autoComplete="off" style={{ maxWidth: 520, display: "block", marginBottom: 12 }} />}
    <Space wrap>
      {!status?.from_env && <Button onClick={() => save(key.trim())} disabled={readOnly || busy || !status || !key.trim()}>儲存金鑰</Button>}
      <Button onClick={test} loading={busy} disabled={readOnly || !status || (!key.trim() && !status.configured)}>測試連線</Button>
      {!status?.from_env && status?.configured && <Popconfirm title="清除已儲存的金鑰？" onConfirm={() => save("")} okText="清除" cancelText="取消">
        <Button danger disabled={readOnly || busy}>清除金鑰</Button>
      </Popconfirm>}
    </Space>
  </section>;
}
