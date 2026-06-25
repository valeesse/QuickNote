UPDATE billing_plans
SET version_history_days = 7,
    updated_at = NOW()
WHERE id = 'free';
