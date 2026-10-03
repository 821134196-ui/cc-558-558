-- 演示种子数据: 两个参展商、五种角色账号、六个展位
INSERT INTO exhibitors (id, name) VALUES
  (1, '华光科技'),
  (2, '星野文创');

INSERT INTO users (id, name, role, exhibitor_id) VALUES
  (1, '王入口', 'receiver',  NULL),
  (2, '李场馆', 'staff',     NULL),
  (3, '赵主管', 'manager',   NULL),
  (4, '陈展商', 'exhibitor', 1),
  (5, '林展商', 'exhibitor', 2);

INSERT INTO booths (id, code, name, hall, owner_exhibitor_id) VALUES
  (1, 'A-01', 'A馆01展位', 'A', 1),
  (2, 'A-02', 'A馆02展位', 'A', 1),
  (3, 'A-03', 'A馆03展位', 'A', 2),
  (4, 'B-01', 'B馆01展位', 'B', 2),
  (5, 'B-02', 'B馆02展位', 'B', NULL),
  (6, 'B-03', 'B馆03展位', 'B', NULL);
